#!/bin/sh
# Keeps data and configuration on removal. A deb purge removes the configuration
# folder (program, site files, backups), but never /var/lib/analyte: it holds
# clinical data, credentials and the admin token, and only people delete those.
set -eu
if [ -d /run/systemd/system ]; then
    systemctl daemon-reload || true
fi
case "${1:-}" in
    purge)
        rm -rf /etc/analyte
        echo "Configuration removed. Clinical data, credentials and the admin token remain in /var/lib/analyte." >&2
        ;;
    remove | 0)
        echo "Analyte removed. Configuration in /etc/analyte and data in /var/lib/analyte were kept." >&2
        ;;
esac
