# NativeLink v1.4.0 on a Debian base, for the remote-execution CI job.
#
# The published image holds the `nativelink` binary and nothing else: no shell and no
# coreutils. Its local worker runs each action as a process inside the container, so an action
# that runs `sh`, `cp` or `cat` could not start there. The binary is linked statically (musl),
# so it is copied as it is onto a base that has those programs.
FROM ghcr.io/tracemachina/nativelink:v1.4.0 AS nativelink

FROM debian:bookworm-slim
COPY --from=nativelink /nix/store/250ajqq06m0jlx1l4rsz75j18lic5yyh-nativelink-x86_64-unknown-linux-musl-1.4.0/bin/nativelink /usr/local/bin/nativelink
ENTRYPOINT ["/usr/local/bin/nativelink"]
