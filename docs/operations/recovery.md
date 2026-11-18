# Protect, recover and upgrade

## Backup

Agree frequency, maximum acceptable data loss, recovery time, protected off-host
target and rotation owner. Analyte provides a verified SQLite backup command,
not a backup scheduler, encryption service or off-host replication system.

```text
analyte --config analyte.toml backup pilot-backup.db
```

The target is relative to the command's working directory unless absolute.
It must not exist. Success prints `Backup verified.` A failure can leave a
partial target; do not restore it. Use this command rather than copying the
live database alone. Backup can run with the service; maintenance excludes
purging while it runs. Check the exit result and copy verified backups to the
agreed protected target. Keep config, approved binaries and secrets separately
in the recovery set. The database backup does not include those files or
external handoff folders. Do not assume it replaces their backup.

## Restore and rehearse

Rehearse on an isolated host with synthetic data before go-live. Give the
rehearsal config its own data directory and local test peers. Prevent access
to production analyzers and app destinations. Record backup age, time to
recover, file permissions, key availability, pause and reconciliation evidence.

1. Stop Analyte and wait for shutdown. Preserve the existing data directory.
   Confirm the selected backup was verified and belongs to this installation.
2. Restore with an explicit actor and reason:

   ```text
   analyte --config analyte.toml --actor pilot-it restore --from pilot-backup.db --reason "Synthetic restore rehearsal"
   ```

3. The program checks integrity and refuses a backup with an older or newer
   schema version. It opens a staged copy and retains the previous database as `analyte.before-restore`. Existing
   recovery files refuse another restore. Archive previous files securely
   before any later restore. Preserve and investigate `analyte.restore-pending`
   after interruption; never delete it merely to bypass startup refusal.
4. Provision the original digest key and other installation credentials.
   Restart. Missing or mismatched digest keys can refuse startup. Result
   receiving and app delivery continue, but **machine order sending and query
   reservations stay paused**. App retries use the original IDs and bytes.
5. Inspect status and `orders list`. Compare the app and machine work lists,
   including all work after the backup. The restored store cannot know what
   reached a machine since then. Correct app requests and reconcile uncertain
   entries using verified evidence. The command form is:

   ```text
   analyte --config analyte.toml --actor pilot-it orders reconcile <local-id> --as accepted --reason "Acceptance verified on machine"
   ```

   `<local-id>` is the local number from `orders list`; replace it rather than
   typing angle brackets. Outcomes are `accepted`, `rejected` or `cancelled`.
   Reconciliation records a decision; it does not issue a machine command.
6. Only after reconciliation, release the pause:

   ```text
   analyte --config analyte.toml --actor pilot-it orders resume-after-restore --reason "Machines and app reconciled"
   ```

   This audited step clears the global pause. It does not itself send a
   command. Normal eligible work can proceed afterwards.

Check duplicate deliveries have one effect in the receiver, uncertain work
was not resent, and the final report agrees. Sign and retain the rehearsal
record. In an actual incident use the same procedure with an incident-specific
reason. Restore supplies a 30-second copy deadline; opening its
staged store uses the storage library's separate default startup budget of
600 seconds. These are not controlled by the configured store startup setting.
Escalate a failed restore with files preserved.

## Retention under ADR 0013

The lab must choose a period in every machine site file. There is no default.
Recommend `retention = 90` days; supported values are 1–3650 or
`retention = "keep"` for no purge. Record the clinical, contractual and privacy
basis for the choice with the lab. It is Analyte's safety copy; the app holds
the clinical record. Shortening a period can make finished data eligible at
the next background run. Approve changes before restart.

A finite period requires `ANALYTE_SECRET_RETENTION_DIGEST`: exactly 64 hex
characters encoding 32 random bytes generated through the site's secure
credential process. Do not use a repeated example key. Keep it outside the
database and repository, with a recoverable protected copy alongside the
installation secrets. Tombstones record its fingerprint. Store opening checks
that fingerprint against the supplied key; mismatch refuses startup. Do not
replace the key or alter the database to bypass this check. Key rotation is
unsupported. Existing tombstones still require their key even after switching
policy to `keep`. Rehearse restoring with the original key; no CLI fingerprint
inspection or key recovery command exists.

### Storage for finite retention

Finite retention needs a store that can return free space bit by bit
(SQLite incremental vacuum). Every store Analyte creates is set up this way.
Startup refuses a finite policy on any other file; restore a verified backup
instead of changing the file by hand.

### What is purged and what stays

Background batches run about every minute with bounded transactions. Only
finished, acknowledged data older than the chosen period is eligible. Open,
uncertain, unresolved, parked or held work prevents purging. The receipt period
runs from its last acknowledgment; terminal orders use their final-state time.
The admin page shows policy, last run and oldest retained receipt. Runs are
audited; backup/restore maintenance pauses purging.

Purging removes original bytes and payloads. Identity tombstones and keyed
duplicate evidence remain. Reprocess and resend then refuse that purged item.
Space reclamation is incremental; do not expect the whole file to shrink at
once. See [ADR 0013](../adr/0013-retention-of-delivered-data.md) for the full eligibility rules.

Place indefinite holds before an investigation, in the admin page or CLI:

```text
analyte --config analyte.toml --actor pilot-it holds receipt <receipt-uuid> --reason "Synthetic investigation"
analyte --config analyte.toml --actor pilot-it holds order demo SYN-ORDER-01 --reason "Synthetic investigation"
```

Replace `<receipt-uuid>` with an actual retained receipt ID. An order hold also
covers linked receipts. Add `--release` with a reason only after the responsible
operator authorizes release. Both transitions are audited. Holds never expire
automatically and cannot recover already purged bytes.

**Older backups still contain purged data until rotated.** Agree backup expiry
with the retention decision. Protect displaced restore stores and incident
copies too. Live-store retention does not erase these copies.

## Upgrade

Schedule downtime with the lab. Review the release notes and evidence for the
exact machine capabilities in use. Verify the new archive as in
[Install](install.md). There is no automatic updater or downgrade command.

1. Make a verified backup and save the current binary, config and secrets.
2. Rehearse the upgrade and restore on an isolated copy. Measure migration time
   and check capacity. Re-run the lab's end-to-end validation after relevant
   changes.
3. Stop the service and wait. Replace the binary and approved configuration.
   Run `check-config`; keep service permissions intact. Follow the packaged
   installer if service settings need updating.
4. Start and inspect logs, admin status, pending delivery, cache trust and
   parked/uncertain work. Startup checks integrity and applies versioned
   migrations within `store.startup_timeout_seconds`. A timeout rolls back the
   active migration; earlier versions may already be committed.
5. If recovery is needed, preserve the failed store and follow the rehearsed
   restore procedure, including reconciliation. Do not run an older binary
   blindly against an upgraded database or resend orders as a rollback step.
