#!/bin/sh
# Stops and disables the service when the package is removed (not upgraded).
# deb passes "remove" or "upgrade"; rpm passes the number of versions left.
set -eu
case "${1:-}" in
    remove | 0)
        if [ -d /run/systemd/system ]; then
            systemctl disable --now analyte.service || true
        fi
        ;;
esac
