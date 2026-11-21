# Pilot installation and operations

Analyte is a bridge. It stores analyzer messages, delivers results to a lab
app, and answers machine queries from orders supplied by that app. It is not
a LIS. It does not verify results, match patients, decide QC outcomes, apply
reflex rules or interpret clinical values. The lab app owns those decisions.

This guide is for lab IT and the named lab operator. Follow these pages in
order. Examples are synthetic. Do not use their values as machine settings.

| Task | Guide |
| --- | --- |
| Requirements, program and service installation | [Install](install.md) |
| Install only the engine and set it up from PowerShell | [Install without the app](install.md#install-without-the-app) |
| Configuration, app connection and machine qualification | [Connect](connect.md) |
| Admin page, alerts and storage incidents | [Operate](operate.md) |
| Who may sign in with their Windows account | [Who may sign in](sign-in.md) |
| Backup, restore, retention and upgrade | [Protect and recover](recovery.md) |
| Record approval before clinical use | [Pilot sign-off](pilot.md) |

Evidence is **documented**, unless a particular capability has separate
recorded evidence. No first-lab machine is qualified today. Installation or
`check-config` success is not permission to use patient results. The six
machines remain blocked pending the facts and recordings in their READMEs.
The lab must validate the complete route through the final clinical report.

Keep a protected installation record outside Git. Record the binary version,
config digest, machine software, changes, responders and approvals. Keep
patient data out of tickets, ordinary logs and repository examples.
