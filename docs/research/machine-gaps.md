# What each machine still lacks

Updated 2026-11-18. This list replaces the "still missing" column of the
earlier document hunt. It merges [plan §9](../plan.md#9-first-machines),
[context "Still needed from outside"](../context.md#still-needed-from-outside)
and the blocked definitions in `machines/`. Anything already obtained is left
out. Documents are named by title, number and version only.

## Per machine

| Machine | Document still missing | Settings and evidence still missing |
| --- | --- | --- |
| Roche cobas e 411 | Host Interface Manual revision for the installed software, with its errata (v2.2 and v2.3 read so far) | Software version; disk or rack model; selected host dialect (Elecsys or cobas) and whether it packs several records in one frame; serial settings including flow control; upload options; the installed host test code screen; a copy of the host trace log; recordings of patient, qualitative, QC, no-barcode and query exchanges |
| YHLO iFlash 1200 | A YHLO LIS protocol manual for the 1200, or YHLO's written statement that LIS Protocol Manual V1.1 (iFlash family) applies to it | Software version; LIS port and connection settings; character encoding; ACK, QCK and DSR response deadlines; LIS assay channel list; evidence for vendor result-status meanings and OBR-only QC groups (config cannot express these yet); recordings of query, result and QC exchanges |
| Boule Swelab Lumi | Boule's LIS protocol description for the Lumi | Nameplate (confirm Lumi) and software version; framing (MLLP or not), encoding and HL7 version; the field that holds the sample ID; lookup message layout; ACK and lookup deadlines; QC marker; recordings with typed, scanned and auto-numbered IDs |
| BioCARE Biolyte 2000 | Service Manual Appendix VI revision for the installed firmware (4.2A read so far) | Firmware version; serial flow control; which entered ID is the lab's sample ID (ID or accession); a reliable QC marker; recordings of patient, QC, flagged and retransferred results |
| bioMérieux VIDAS (PC) | BCI Link LIS Interface Driver Development Guide 161150-486, the revision for the installed VIDAS PC and BCI version (A and B used so far) | VIDAS PC version (decides a 12 or 17 character ID); BCI option enabled; full serial settings including flow control; Literal or Alternate mode; assay short codes; recordings of uploads and a work list download |
| Dialab Autolyser (REF 20010D) | Dialab's TCP/IP host communication supplement; the approved barcode reader retrofit kit and procedure for REF 20010D (if one exists) | Installed software version (Operator's Manual Rev18 covers 21.18); all serial parameters, including data bits; internal or variable protocol; manual or automatic report release; non-calculable (NC) transmission enabled; four-byte analysis codes; recordings of NC results and of polling with manual and automatic release |

## For every machine

- Which tests run on which machine, and the lab app's code for each.
- One de-identified recording per machine before its config moves past
  documented.

## Local copies

These source documents were read during research but have no copy in the
git-ignored `references/vendors/` folder. Builders need them there:

- cobas e 411: Host Interface Manual v2.3 (and v2.2); Operator's Manual v3.3.1.
- iFlash 1200: YHLO LIS Protocol Manual V1.1; iFlash 1200 Operation Manual V3.0.
- Swelab Lumi: Lumi User Manual 1504529.
- Biolyte 2000: Service Manual 4.2A.
- VIDAS: BCI Link guide 161150-486 revisions A and B; VIDAS PC User Manual F.

The Autolyser Operator's Manual Rev18 and Service Manual Rev01 are present.
