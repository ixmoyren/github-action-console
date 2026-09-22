# CI builds and uploads release assets; the console only triggers and observes

The console must surface Intel/Arm macOS and Windows downloads from GitHub Releases, but octocrab's `upload_asset` requires the entire file in memory with no streaming, and signing/notarization toolchains belong on a build machine, not a user's desktop. Building, signing and uploading therefore stay on GitHub Actions runners: the console triggers workflows, polls status, and surfaces `browser_download_url`s, and never handles large binary payloads itself. Considered building and uploading from the desktop
app (rejected: memory-bound uploads, local toolchain burden) and a hybrid split (rejected: two divergent upload paths).
