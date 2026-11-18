#!/bin/sh
# Creates the analyte system account before the package's files are unpacked,
# so they can be owned by it. Runs for .deb and .rpm installs and upgrades.
set -eu
if ! getent group analyte >/dev/null; then
    groupadd --system analyte
fi
if ! id analyte >/dev/null 2>&1; then
    useradd --system --gid analyte --home-dir /var/lib/analyte --no-create-home \
        --shell /usr/sbin/nologin --comment "Analyte clinical analyzer bridge" analyte
fi
