//! Moving blobs to and from the remote cache.
//!
//! A blob that fits under the batch threshold travels in a `BatchReadBlobs` or
//! `BatchUpdateBlobs` call with others; a larger one goes through `ByteStream`, read in the
//! cache's own message sizes and written in 1 MiB chunks. [`find_missing`] asks which of some
//! blobs the cache lacks.
//!
//! A call is bounded whole by the client's timeout. A `ByteStream` read allows each message the
//! timeout, so a large blob that keeps arriving is never cut off, and a write allows it for
//! each chunk it sends.

use std::collections::BTreeMap;

use tonic::Code;
use tonic::codegen::tokio_stream;
use uuid::Uuid;

use super::client::Wire;
use super::error::RemoteError;
use super::proto::build::bazel::remote::execution::v2::{
    BatchReadBlobsRequest, BatchUpdateBlobsRequest, Digest, FindMissingBlobsRequest,
    batch_update_blobs_request, digest_function,
};
use super::proto::google::bytestream::{ReadRequest, WriteRequest};
use super::proto::google::rpc;

/// The size of a chunk written through `ByteStream`.
const WRITE_CHUNK: usize = 1024 * 1024;

/// The most memory reserved up front for a blob read through `ByteStream`, whatever size the
/// digest claims.
const READ_RESERVE: usize = 16 * 1024 * 1024;

const BATCH_READ: &str = "ContentAddressableStorage.BatchReadBlobs";
const BATCH_UPDATE: &str = "ContentAddressableStorage.BatchUpdateBlobs";
const STREAM_READ: &str = "ByteStream.Read";
const STREAM_WRITE: &str = "ByteStream.Write";

/// How a set of blobs divides between batch calls and `ByteStream`.
#[derive(Debug, PartialEq, Eq)]
struct Split {
    /// Positions of the blobs in each batch, in order; a batch holds at most the threshold.
    batches: Vec<Vec<usize>>,
    /// Positions of the blobs too large for any batch.
    streamed: Vec<usize>,
}

/// Divides blobs of `sizes` between batches of at most `threshold` bytes and `ByteStream`.
///
/// Blobs join the open batch in order, and a blob that would carry it over the threshold opens
/// the next one. A blob larger than the threshold on its own is streamed.
fn split(sizes: &[usize], threshold: usize) -> Split {
    let mut batches = Vec::new();
    let mut streamed = Vec::new();
    let mut open = Vec::new();
    let mut total = 0_usize;
    for (position, &size) in sizes.iter().enumerate() {
        if size > threshold {
            streamed.push(position);
            continue;
        }
        if total.saturating_add(size) > threshold {
            batches.push(std::mem::take(&mut open));
            total = 0;
        }
        open.push(position);
        total += size;
    }
    if !open.is_empty() {
        batches.push(open);
    }
    Split { batches, streamed }
}

/// `rest` under `instance`, or alone when no instance is set.
fn under_instance(instance: &str, rest: &str) -> String {
    if instance.is_empty() {
        rest.to_owned()
    } else {
        format!("{instance}/{rest}")
    }
}

/// The `ByteStream` resource name a blob is read from.
fn read_resource(instance: &str, digest: &Digest) -> String {
    under_instance(
        instance,
        &format!("blobs/{}/{}", digest.hash, digest.size_bytes),
    )
}

/// The `ByteStream` resource name a blob is written to, under the upload named `upload`.
fn write_resource(instance: &str, upload: Uuid, digest: &Digest) -> String {
    under_instance(
        instance,
        &format!(
            "uploads/{upload}/blobs/{}/{}",
            digest.hash, digest.size_bytes
        ),
    )
}

/// The digest as `hash/size`, for messages.
fn label(digest: &Digest) -> String {
    format!("{}/{}", digest.hash, digest.size_bytes)
}

/// The size a digest states, or the protocol error a negative one is.
fn size_of(digest: &Digest, call: &'static str) -> Result<usize, RemoteError> {
    usize::try_from(digest.size_bytes).map_err(|_| RemoteError::Protocol {
        call,
        detail: format!("the digest {} has a negative size", label(digest)),
    })
}

/// The failure of one blob, as the status the cache gave it.
fn blob_failure(digest: &Digest, code: Code, message: &str) -> RemoteError {
    RemoteError::Blob {
        digest: label(digest),
        code,
        message: message.to_owned(),
    }
}

/// Re-reads a failed status of a call about `digest` as the failure of that blob.
fn naming(digest: &Digest) -> impl FnOnce(RemoteError) -> RemoteError + '_ {
    move |error| match error {
        RemoteError::Status { source, .. } => blob_failure(digest, source.code(), source.message()),
        other => other,
    }
}

/// Fails with the blob's status when `status` is not OK; an absent status is OK.
fn check(digest: &Digest, status: Option<&rpc::Status>) -> Result<(), RemoteError> {
    status.map_or(Ok(()), |status| match Code::from(status.code) {
        Code::Ok => Ok(()),
        code => Err(blob_failure(digest, code, &status.message)),
    })
}

/// The members of `digests` the cache does not hold.
///
/// # Errors
///
/// Returns the call's failure.
pub(super) async fn find_missing(
    wire: &Wire<'_>,
    digests: &[Digest],
) -> Result<Vec<Digest>, RemoteError> {
    let mut client = wire.cas();
    let request = FindMissingBlobsRequest {
        instance_name: wire.instance().to_owned(),
        blob_digests: digests.to_vec(),
        digest_function: digest_function::Value::Sha256 as i32,
    };
    let response = wire
        .bounded(
            "ContentAddressableStorage.FindMissingBlobs",
            client.find_missing_blobs(request),
        )
        .await?;
    Ok(response.into_inner().missing_blob_digests)
}

/// Reads the blobs `digests` name, in the same order.
///
/// # Errors
///
/// Returns the failure of a call, or of the first blob that failed.
pub(super) async fn read_blobs(
    wire: &Wire<'_>,
    threshold: usize,
    digests: &[Digest],
) -> Result<Vec<Vec<u8>>, RemoteError> {
    let sizes = digests
        .iter()
        .map(|digest| size_of(digest, BATCH_READ))
        .collect::<Result<Vec<_>, _>>()?;
    let split = split(&sizes, threshold);

    let mut read = BTreeMap::new();
    for batch in split.batches {
        let wanted: Vec<_> = batch.iter().filter_map(|at| digests.get(*at)).collect();
        let blobs = batch_read(wire, &wanted).await?;
        read.extend(batch.into_iter().zip(blobs));
    }
    for at in split.streamed {
        if let Some(digest) = digests.get(at) {
            read.insert(at, stream_read(wire, digest).await?);
        }
    }
    (0..digests.len())
        .map(|at| {
            read.remove(&at).ok_or_else(|| RemoteError::Protocol {
                call: BATCH_READ,
                detail: "a requested blob was not read".to_owned(),
            })
        })
        .collect()
}

/// Reads `digests`, which together fit one batch, with one call.
async fn batch_read(wire: &Wire<'_>, digests: &[&Digest]) -> Result<Vec<Vec<u8>>, RemoteError> {
    let mut client = wire.cas();
    let request = BatchReadBlobsRequest {
        instance_name: wire.instance().to_owned(),
        digests: digests.iter().map(|digest| (*digest).clone()).collect(),
        digest_function: digest_function::Value::Sha256 as i32,
        ..BatchReadBlobsRequest::default()
    };
    let response = wire
        .bounded(BATCH_READ, client.batch_read_blobs(request))
        .await?
        .into_inner();

    let mut answers = BTreeMap::new();
    for answer in response.responses {
        if let Some(digest) = &answer.digest {
            answers.insert((digest.hash.clone(), digest.size_bytes), answer);
        }
    }
    digests
        .iter()
        .map(|digest| {
            let answer = answers
                .remove(&(digest.hash.clone(), digest.size_bytes))
                .ok_or_else(|| RemoteError::Protocol {
                    call: BATCH_READ,
                    detail: format!("no answer for the blob {}", label(digest)),
                })?;
            check(digest, answer.status.as_ref())?;
            if usize::try_from(digest.size_bytes) != Ok(answer.data.len()) {
                return Err(RemoteError::Protocol {
                    call: BATCH_READ,
                    detail: format!(
                        "the blob {} came back with {} bytes",
                        label(digest),
                        answer.data.len()
                    ),
                });
            }
            Ok(answer.data)
        })
        .collect()
}

/// Reads one blob through `ByteStream`, allowing each message the client's timeout.
async fn stream_read(wire: &Wire<'_>, digest: &Digest) -> Result<Vec<u8>, RemoteError> {
    let size = size_of(digest, STREAM_READ)?;
    let mut client = wire.bytestream();
    let request = ReadRequest {
        resource_name: read_resource(wire.instance(), digest),
        read_offset: 0,
        read_limit: 0,
    };
    let mut stream = wire
        .bounded(STREAM_READ, client.read(request))
        .await
        .map_err(naming(digest))?
        .into_inner();

    let mut data = Vec::with_capacity(size.min(READ_RESERVE));
    while let Some(message) = wire
        .bounded(STREAM_READ, stream.message())
        .await
        .map_err(naming(digest))?
    {
        data.extend(message.data);
        if data.len() > size {
            break;
        }
    }
    if data.len() == size {
        Ok(data)
    } else {
        Err(RemoteError::Protocol {
            call: STREAM_READ,
            detail: format!(
                "the blob {} came back with {} bytes or more",
                label(digest),
                data.len()
            ),
        })
    }
}

/// Writes each blob of `blobs`, which the caller has checked the cache accepts.
///
/// # Errors
///
/// Returns the failure of a call, or of the first blob that failed.
pub(super) async fn write_blobs(
    wire: &Wire<'_>,
    threshold: usize,
    blobs: &[(Digest, Vec<u8>)],
) -> Result<(), RemoteError> {
    let sizes: Vec<_> = blobs.iter().map(|(_, data)| data.len()).collect();
    let split = split(&sizes, threshold);

    for batch in split.batches {
        let batch: Vec<_> = batch.iter().filter_map(|at| blobs.get(*at)).collect();
        batch_write(wire, &batch).await?;
    }
    for at in split.streamed {
        if let Some((digest, data)) = blobs.get(at) {
            stream_write(wire, digest, data).await?;
        }
    }
    Ok(())
}

/// Writes `blobs`, which together fit one batch, with one call.
async fn batch_write(wire: &Wire<'_>, blobs: &[&(Digest, Vec<u8>)]) -> Result<(), RemoteError> {
    let mut client = wire.cas();
    let request = BatchUpdateBlobsRequest {
        instance_name: wire.instance().to_owned(),
        requests: blobs
            .iter()
            .map(|(digest, data)| batch_update_blobs_request::Request {
                digest: Some(digest.clone()),
                data: data.clone(),
                ..batch_update_blobs_request::Request::default()
            })
            .collect(),
        digest_function: digest_function::Value::Sha256 as i32,
    };
    let response = wire
        .bounded(BATCH_UPDATE, client.batch_update_blobs(request))
        .await?
        .into_inner();

    for answer in &response.responses {
        if let Some(digest) = &answer.digest {
            check(digest, answer.status.as_ref())?;
        }
    }
    if response.responses.len() < blobs.len() {
        return Err(RemoteError::Protocol {
            call: BATCH_UPDATE,
            detail: format!(
                "{} answers for {} blobs",
                response.responses.len(),
                blobs.len()
            ),
        });
    }
    Ok(())
}

/// Writes one blob through `ByteStream` in 1 MiB chunks under a fresh upload name.
///
/// The call is allowed the client's timeout for each chunk it sends.
async fn stream_write(wire: &Wire<'_>, digest: &Digest, data: &[u8]) -> Result<(), RemoteError> {
    let name = write_resource(wire.instance(), Uuid::new_v4(), digest);
    let pieces: Vec<&[u8]> = if data.is_empty() {
        vec![data]
    } else {
        data.chunks(WRITE_CHUNK).collect()
    };
    let allowance = wire
        .timeout()
        .saturating_mul(u32::try_from(pieces.len()).unwrap_or(u32::MAX));
    let last = pieces.len().saturating_sub(1);
    let messages: Vec<_> = pieces
        .into_iter()
        .enumerate()
        .map(|(index, piece)| WriteRequest {
            resource_name: if index == 0 {
                name.clone()
            } else {
                String::new()
            },
            write_offset: i64::try_from(index.saturating_mul(WRITE_CHUNK)).unwrap_or(i64::MAX),
            finish_write: index == last,
            data: piece.to_vec(),
        })
        .collect();

    let mut client = wire.bytestream();
    let response = wire
        .bounded_for(
            STREAM_WRITE,
            allowance,
            client.write(tokio_stream::iter(messages)),
        )
        .await
        .map_err(naming(digest))?
        .into_inner();
    if usize::try_from(response.committed_size) == Ok(data.len()) {
        Ok(())
    } else {
        Err(RemoteError::Protocol {
            call: STREAM_WRITE,
            detail: format!(
                "the cache committed {} bytes of the blob {}",
                response.committed_size,
                label(digest)
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::{Split, read_resource, split, write_resource};
    use crate::remote::proto::build::bazel::remote::execution::v2::Digest;

    fn digest() -> Digest {
        Digest {
            hash: "ab".repeat(32),
            size_bytes: 7,
        }
    }

    #[test]
    fn blobs_within_the_threshold_share_a_batch() {
        assert_eq!(
            split(&[10, 20, 30], 100),
            Split {
                batches: vec![vec![0, 1, 2]],
                streamed: vec![],
            }
        );
    }

    #[test]
    fn a_batch_closes_when_the_next_blob_would_pass_the_threshold() {
        assert_eq!(
            split(&[60, 40, 1, 99], 100),
            Split {
                batches: vec![vec![0, 1], vec![2, 3]],
                streamed: vec![],
            }
        );
    }

    #[test]
    fn a_blob_over_the_threshold_alone_is_streamed() {
        assert_eq!(
            split(&[10, 101, 10], 100),
            Split {
                batches: vec![vec![0, 2]],
                streamed: vec![1],
            }
        );
    }

    #[test]
    fn a_blob_at_the_threshold_is_batched() {
        assert_eq!(
            split(&[100], 100),
            Split {
                batches: vec![vec![0]],
                streamed: vec![],
            }
        );
    }

    #[test]
    fn no_blobs_make_no_calls() {
        assert_eq!(
            split(&[], 100),
            Split {
                batches: vec![],
                streamed: vec![],
            }
        );
    }

    #[test]
    fn resource_names_follow_the_instance() {
        let digest = digest();
        let upload = Uuid::nil();
        let hash = &digest.hash;

        assert_eq!(read_resource("ci", &digest), format!("ci/blobs/{hash}/7"));
        assert_eq!(read_resource("", &digest), format!("blobs/{hash}/7"));
        assert_eq!(
            write_resource("ci", upload, &digest),
            format!("ci/uploads/{upload}/blobs/{hash}/7")
        );
        assert_eq!(
            write_resource("", upload, &digest),
            format!("uploads/{upload}/blobs/{hash}/7")
        );
    }
}
