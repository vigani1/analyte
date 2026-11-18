#!/bin/sh
# Protects the folders, creates the admin token when missing, then enables and
# (re)starts the service. Never prints the token: package logs keep this output.
set -eu
config=/etc/analyte/analyte.toml
# The setup page writes the program file, site files and backups here.
chown root:analyte /etc/analyte
chmod 0770 /etc/analyte
chown analyte:analyte /var/lib/analyte
chmod 0700 /var/lib/analyte
if [ ! -e /var/lib/analyte/admin-token ]; then
    # Written mode 0600 by the service account, which must be able to read it.
    runuser -u analyte -- /usr/bin/analyte --config "$config" admin token --rotate >/dev/null
fi
# The [admin] port of the program file, if the page is enabled.
port=$(awk '/^\[/ { admin = ($0 ~ /^\[admin\][[:space:]]*$/) } admin && /^port[[:space:]]*=/ { sub(/^port[[:space:]]*=[[:space:]]*/, ""); sub(/[^0-9].*$/, ""); print; exit }' "$config")
if [ -d /run/systemd/system ]; then
    systemctl daemon-reload
    systemctl enable analyte.service
    if ! systemctl restart analyte.service; then
        echo "Analyte did not start. See: sudo journalctl -u analyte.service" >&2
    fi
else
    echo "systemd is not running; start Analyte with: sudo systemctl enable --now analyte.service" >&2
fi
echo "Analyte is installed. Configuration: /etc/analyte; data: /var/lib/analyte (kept on removal)."
if [ -n "$port" ]; then
    echo "Set up machines at http://127.0.0.1:$port/setup on this computer."
fi
echo "Admin token for the login page: sudo cat /var/lib/analyte/admin-token"
