# NativeLink v1.4.0 on a Debian base with python3, the server the remote example runs on.
#
# The published image holds the `nativelink` binary and nothing else. Its local worker runs
# each action as a process inside the container, so the program an action names must be
# there: this image adds python3. The binary is linked statically (musl), so it is copied as it
# is. Both images are pinned by digest, since the copied path names the binary's store hash in
# the NativeLink image.
FROM ghcr.io/tracemachina/nativelink:v1.4.0@sha256:56e4a01ebb3cc58c984627f9f284746b48b29abc95ea4363b8720bc8da5e50d4 AS nativelink

FROM debian:bookworm-slim@sha256:7ff6ba31527b9e336162fa061746c366c4b2032f4d2cff2078bea43f78efe3f8
RUN apt-get update \
    && apt-get install -y --no-install-recommends python3 \
    && rm -rf /var/lib/apt/lists/*
COPY --from=nativelink /nix/store/250ajqq06m0jlx1l4rsz75j18lic5yyh-nativelink-x86_64-unknown-linux-musl-1.4.0/bin/nativelink /usr/local/bin/nativelink
ENTRYPOINT ["/usr/local/bin/nativelink"]
