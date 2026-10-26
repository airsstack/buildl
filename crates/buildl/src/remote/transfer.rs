//! Moving blobs to and from the remote cache.
//!
//! A blob that fits under the batch threshold travels in a `BatchReadBlobs` or
//! `BatchUpdateBlobs` call with others; a larger one goes through `ByteStream`, read in the
//! cache's own message sizes and written in 1 MiB chunks. [`find_missing`] asks which of some
//! blobs the cache lacks.
//!
//! A call is bounded whole by the client's timeout. A `ByteStream` read allows each message the
//! timeout, so a large blob that keeps arriving is never cut off, and a write allows the
//! timeout from the moment the transport last took a chunk, so a large blob that keeps being
//! taken is never cut off either.

use std::collections::{BTreeMap, BTreeSet};

use tonic::Code;
use tonic::codegen::tokio_stream::{self, StreamExt};
use uuid::Uuid;

use super::capabilities::{HEADROOM, MESSAGE_CAP};
use super::client::{Progress, Wire};
use super::error::RemoteError;
use super::proto::build::bazel::remote::execution::v2::{
    BatchReadBlobsRequest, BatchUpdateBlobsRequest, Digest, FindMissingBlobsRequest,
    batch_update_blobs_request, digest_function,
};
use super::proto::google::bytestream::{ReadRequest, WriteRequest};
use super::proto::google::rpc;
use super::vocabulary::Method;

/// The size of a chunk written through `ByteStream`.
const WRITE_CHUNK: usize = 1024 * 1024;

/// The most memory reserved up front for a blob read through `ByteStream`, whatever size the
/// digest claims.
const READ_RESERVE: usize = 16 * 1024 * 1024;

/// What the largest message of a batch call may weigh on the wire, entries' framing included.
const BATCH_BUDGET: usize = MESSAGE_CAP - HEADROOM;

/// What one entry of a batch call weighs besides its data: the digest (64 hex digits and a
/// size), the status, and the field framing around them. The server's own limit counts data
/// only; this keeps a batch of many small blobs under the message cap all the same.
const ENTRY_OVERHEAD: usize = 128;

/// How a set of blobs divides between batch calls and `ByteStream`.
#[derive(Debug, PartialEq, Eq)]
struct Split {
    /// Positions of the blobs in each batch, in order; a batch holds at most the threshold in
    /// data and at most the batch budget on the wire.
    batches: Vec<Vec<usize>>,
    /// Positions of the blobs too large for any batch.
    streamed: Vec<usize>,
}

/// Divides blobs of `sizes` between batches of at most `threshold` bytes of data and
/// `ByteStream`.
///
/// Blobs join the open batch in order, and a blob that would carry it over the threshold, or
/// carry its encoded size over the batch budget once each entry's framing is counted, opens
/// the next one. A blob larger than the threshold on its own is streamed.
fn split(sizes: &[usize], threshold: usize) -> Split {
    let mut batches = Vec::new();
    let mut streamed = Vec::new();
    let mut open = Vec::new();
    let mut total = 0_usize;
    let mut framed = 0_usize;
    for (position, &size) in sizes.iter().enumerate() {
        if size > threshold {
            streamed.push(position);
            continue;
        }
        let entry = size.saturating_add(ENTRY_OVERHEAD);
        if !open.is_empty()
            && (total.saturating_add(size) > threshold
                || framed.saturating_add(entry) > BATCH_BUDGET)
        {
            batches.push(std::mem::take(&mut open));
            total = 0;
            framed = 0;
        }
        open.push(position);
        total += size;
        framed += entry;
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

/// The key a digest is looked up by.
fn key(digest: &Digest) -> (String, i64) {
    (digest.hash.clone(), digest.size_bytes)
}

/// The size a digest states, or the protocol error a negative one is, named for `call`.
fn size_of(digest: &Digest, call: Method) -> Result<usize, RemoteError> {
    usize::try_from(digest.size_bytes).map_err(|_| RemoteError::Protocol {
        call,
        detail: format!("the digest {} has a negative size", label(digest)),
    })
}

/// The failure of one blob carried by `call`, as the status the cache gave it.
fn blob_failure(call: Method, digest: &Digest, code: Code, message: &str) -> RemoteError {
    RemoteError::Blob {
        call,
        digest: digest.clone(),
        code,
        message: message.to_owned(),
    }
}

/// Re-reads a failed status of a call about `digest` as the failure of that blob.
fn naming(digest: &Digest) -> impl FnOnce(RemoteError) -> RemoteError + '_ {
    move |error| match error {
        RemoteError::Status { call, source } => {
            blob_failure(call, digest, source.code(), source.message())
        }
        other => other,
    }
}

/// Fails with the blob's status in the answer to `call` when `status` is not OK; an absent
/// status is OK.
fn check(call: Method, digest: &Digest, status: Option<&rpc::Status>) -> Result<(), RemoteError> {
    status.map_or(Ok(()), |status| match Code::from(status.code) {
        Code::Ok => Ok(()),
        code => Err(blob_failure(call, digest, code, &status.message)),
    })
}

/// What one digest of a `FindMissingBlobs` request weighs at most on the wire: 64 hex digits and
/// a size of up to ten varint bytes, with the framing of the digest and of its place in the list.
const FIND_ENTRY: usize = 80;

/// The most digests one `FindMissingBlobs` request carries, so that it stays under the batch
/// budget.
const FIND_CHUNK: usize = BATCH_BUDGET / FIND_ENTRY;

/// The members of `digests` the cache does not hold, in the order they were given.
///
/// The digests go to the cache in as many calls as it takes to keep each request under the
/// message cap.
///
/// # Errors
///
/// Returns the failure of a call.
pub(super) async fn find_missing(
    wire: &Wire<'_>,
    digests: &[Digest],
) -> Result<Vec<Digest>, RemoteError> {
    let mut missing = Vec::new();
    for chunk in digests.chunks(FIND_CHUNK) {
        let mut client = wire.cas();
        let request = FindMissingBlobsRequest {
            instance_name: wire.instance().to_owned(),
            blob_digests: chunk.to_vec(),
            digest_function: digest_function::Value::Sha256 as i32,
        };
        let response = wire
            .bounded(Method::FindMissingBlobs, client.find_missing_blobs(request))
            .await?;
        missing.extend(response.into_inner().missing_blob_digests);
    }
    Ok(missing)
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
    // Every read starts as a batch read, so a size that cannot be one is named for that call.
    let sizes = digests
        .iter()
        .map(|digest| size_of(digest, Method::BatchReadBlobs))
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
                call: Method::BatchReadBlobs,
                detail: "a requested blob was not read".to_owned(),
            })
        })
        .collect()
}

/// Reads `digests`, which together fit one batch, with one call.
///
/// A digest that appears more than once is asked for once and answered at each position.
async fn batch_read(wire: &Wire<'_>, digests: &[&Digest]) -> Result<Vec<Vec<u8>>, RemoteError> {
    let mut uses: BTreeMap<(String, i64), usize> = BTreeMap::new();
    let mut unique = Vec::new();
    for digest in digests {
        let count = uses.entry(key(digest)).or_default();
        if *count == 0 {
            unique.push((*digest).clone());
        }
        *count += 1;
    }

    let mut client = wire.cas();
    let request = BatchReadBlobsRequest {
        instance_name: wire.instance().to_owned(),
        digests: unique.clone(),
        digest_function: digest_function::Value::Sha256 as i32,
        ..BatchReadBlobsRequest::default()
    };
    let response = wire
        .bounded(Method::BatchReadBlobs, client.batch_read_blobs(request))
        .await?
        .into_inner();

    let mut answers = BTreeMap::new();
    for answer in response.responses {
        if let Some(digest) = &answer.digest {
            answers.insert(key(digest), answer);
        }
    }
    let mut blobs = BTreeMap::new();
    for digest in &unique {
        let answer = answers
            .remove(&key(digest))
            .ok_or_else(|| RemoteError::Protocol {
                call: Method::BatchReadBlobs,
                detail: format!("no answer for the blob {}", label(digest)),
            })?;
        check(Method::BatchReadBlobs, digest, answer.status.as_ref())?;
        if usize::try_from(digest.size_bytes) != Ok(answer.data.len()) {
            return Err(RemoteError::Protocol {
                call: Method::BatchReadBlobs,
                detail: format!(
                    "the blob {} came back with {} bytes",
                    label(digest),
                    answer.data.len()
                ),
            });
        }
        blobs.insert(key(digest), answer.data);
    }
    digests
        .iter()
        .map(|digest| {
            let wanted = key(digest);
            let left = uses.entry(wanted.clone()).or_default();
            *left = left.saturating_sub(1);
            let blob = if *left == 0 {
                blobs.remove(&wanted)
            } else {
                blobs.get(&wanted).cloned()
            };
            blob.ok_or_else(|| RemoteError::Protocol {
                call: Method::BatchReadBlobs,
                detail: format!("no answer for the blob {}", label(digest)),
            })
        })
        .collect()
}

/// Reads one blob through `ByteStream`, allowing each message the client's timeout.
async fn stream_read(wire: &Wire<'_>, digest: &Digest) -> Result<Vec<u8>, RemoteError> {
    let size = size_of(digest, Method::Read)?;
    let mut client = wire.bytestream();
    let request = ReadRequest {
        resource_name: read_resource(wire.instance(), digest),
        read_offset: 0,
        read_limit: 0,
    };
    let mut stream = wire
        .bounded(Method::Read, client.read(request))
        .await
        .map_err(naming(digest))?
        .into_inner();

    let mut data = Vec::with_capacity(size.min(READ_RESERVE));
    while let Some(message) = wire
        .bounded(Method::Read, stream.message())
        .await
        .map_err(naming(digest))?
    {
        data.extend(message.data);
        if data.len() > size {
            break;
        }
    }
    match data.len().cmp(&size) {
        std::cmp::Ordering::Equal => Ok(data),
        std::cmp::Ordering::Less => Err(RemoteError::Protocol {
            call: Method::Read,
            detail: format!(
                "the blob {} ended after {} of its {size} bytes",
                label(digest),
                data.len()
            ),
        }),
        std::cmp::Ordering::Greater => Err(RemoteError::Protocol {
            call: Method::Read,
            detail: format!(
                "the blob {} is longer than the {size} bytes its digest states",
                label(digest)
            ),
        }),
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
        .bounded(Method::BatchUpdateBlobs, client.batch_update_blobs(request))
        .await?
        .into_inner();

    let mut answered = BTreeSet::new();
    for answer in &response.responses {
        if let Some(digest) = &answer.digest {
            check(Method::BatchUpdateBlobs, digest, answer.status.as_ref())?;
            answered.insert(key(digest));
        }
    }
    for (digest, _) in blobs.iter().copied() {
        if !answered.contains(&key(digest)) {
            return Err(RemoteError::Protocol {
                call: Method::BatchUpdateBlobs,
                detail: format!("no answer for the blob {}", label(digest)),
            });
        }
    }
    Ok(())
}

/// Writes one blob through `ByteStream` in 1 MiB chunks under a fresh upload name.
///
/// The call is allowed the client's timeout from the moment the transport last took a chunk.
async fn stream_write(wire: &Wire<'_>, digest: &Digest, data: &[u8]) -> Result<(), RemoteError> {
    let name = write_resource(wire.instance(), Uuid::new_v4(), digest);
    let pieces: Vec<&[u8]> = if data.is_empty() {
        vec![data]
    } else {
        data.chunks(WRITE_CHUNK).collect()
    };
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

    let progress = Progress::start();
    let taken = progress.clone();
    let outbound = tokio_stream::iter(messages).map(move |message| {
        taken.record();
        message
    });
    let mut client = wire.bytestream();
    let response = wire
        .bounded_by_progress(Method::Write, &progress, client.write(outbound))
        .await
        .map_err(naming(digest))?
        .into_inner();
    if usize::try_from(response.committed_size) == Ok(data.len()) {
        Ok(())
    } else {
        Err(RemoteError::Protocol {
            call: Method::Write,
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
    use prost::Message;
    use tonic::Code;
    use uuid::Uuid;

    use super::{
        BATCH_BUDGET, ENTRY_OVERHEAD, FIND_CHUNK, Split, blob_failure, read_resource, split,
        write_resource,
    };
    use crate::remote::capabilities::MESSAGE_CAP;
    use crate::remote::proto::build::bazel::remote::execution::v2::{
        Digest, FindMissingBlobsRequest, batch_read_blobs_response, batch_update_blobs_request,
        digest_function,
    };
    use crate::remote::proto::google::rpc;
    use crate::remote::vocabulary::Method;

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

    #[test]
    fn many_small_blobs_close_a_batch_before_its_entries_pass_the_budget() {
        let sizes = vec![300; 12_000];

        let divided = split(&sizes, BATCH_BUDGET);

        assert!(divided.batches.len() > 1, "{}", divided.batches.len());
        assert_eq!(divided.streamed, Vec::<usize>::new());
        for batch in &divided.batches {
            assert!(batch.len() * (300 + ENTRY_OVERHEAD) <= BATCH_BUDGET);
        }
        let positions: Vec<_> = divided.batches.concat();
        assert_eq!(positions, (0..12_000).collect::<Vec<_>>());
    }

    #[test]
    fn empty_blobs_do_not_keep_a_batch_open_for_ever() {
        let divided = split(&vec![0; 100_000], BATCH_BUDGET);

        assert!(divided.batches.len() > 1, "{}", divided.batches.len());
    }

    #[test]
    fn a_blob_filling_the_budget_alone_still_batches() {
        let divided = split(&[BATCH_BUDGET], BATCH_BUDGET);

        assert_eq!(
            divided,
            Split {
                batches: vec![vec![0]],
                streamed: vec![],
            }
        );
    }

    #[test]
    fn a_full_find_missing_request_stays_under_the_batch_budget() {
        let largest = Digest {
            hash: "f".repeat(64),
            size_bytes: i64::MAX,
        };
        let request = FindMissingBlobsRequest {
            instance_name: "i".repeat(256),
            blob_digests: vec![largest; FIND_CHUNK],
            digest_function: digest_function::Value::Sha256 as i32,
        };

        assert!(request.encoded_len() <= BATCH_BUDGET + 256 + 8);
        assert!(request.encoded_len() < MESSAGE_CAP);
    }

    #[test]
    fn a_blob_failure_names_the_call_that_carried_it() {
        let error = blob_failure(Method::Write, &digest(), Code::Aborted, "stop");

        assert_eq!(
            error.to_string(),
            format!(
                "ByteStream.Write: blob {}/7 failed with Aborted: stop",
                digest().hash
            )
        );
    }

    #[test]
    fn the_entry_overhead_covers_the_framing_of_an_entry() {
        let data = vec![0_u8; 4 * 1024 * 1024];
        let digest = Digest {
            hash: "f".repeat(64),
            size_bytes: i64::try_from(data.len()).unwrap_or(i64::MAX),
        };
        let answer = batch_read_blobs_response::Response {
            digest: Some(digest.clone()),
            data: data.clone(),
            status: Some(rpc::Status::default()),
            ..batch_read_blobs_response::Response::default()
        };
        let request = batch_update_blobs_request::Request {
            digest: Some(digest),
            data: data.clone(),
            ..batch_update_blobs_request::Request::default()
        };

        // One entry of a repeated field costs its encoding and the tag and length before it.
        let framed = |encoded: usize| encoded + 1 + prost::length_delimiter_len(encoded);

        assert!(framed(answer.encoded_len()) - data.len() <= ENTRY_OVERHEAD);
        assert!(framed(request.encoded_len()) - data.len() <= ENTRY_OVERHEAD);
    }
}
