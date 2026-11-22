# Biolyte 2000 — low-priority connector research

1. Identity: BioCare Corporation, Taiwan; no verified EasyLyte or Serachem OEM relationship.
2. Parameters: Na/K/Cl; lithium exists in historical configurations; calcium on this unit is UNVERIFIED.
3. Interface: documented 9600 8N1 RS232 with ASTM, not merely a printer-text dump.
4. Feasibility: serial ASTM is promising; patient matching and QC exclusion still need real captures.
5. Recommendation: keep typing three values for now; revisit after obtaining ports/manual photos and de-identified output.

Research date: 2026-11-14. Research only; no device tested or integration implemented.

## Identity and OEM

The manufacturer lists BIOLYTE 2000 among its products. FDA 510(k) K002855 names Biocare and a Na/K/Cl–Li analyzer. Confidence: high for brand/model and historical lithium availability, not the installed electrode configuration.

The V4.2A reference manual, title page, identifies BioCare in Taoyuan, Taiwan. Its electrode selection covers Na/K/Cl/Li. Confidence: medium-high, manufacturer-authored; confirm firmware against the lab's copy. The screen text and function keys seen on the lab's unit match this model's manual closely.

No evidence found establishing Medica EasyLyte or another Taiwanese/Chinese OEM origin: **UNVERIFIED**, visual resemblance is insufficient. One reseller description links the Biolyte name to Serachem, but it describes a touchscreen unit. Confidence: low for this keypad model; do not transfer its scanner/interface claims. BioCare's separate Biolyte V brochure lists Ca/pH: this does not establish calcium capability on the 2000.

## RS232 and exact output evidence

Service Manual 4.2A, pp. 13–14 and 80–86, documents:

- DB25 female; 9600 baud, 8 data bits, no parity, one stop bit.
- Pins: 2 Rx input, 3 Tx output, 4 CTS input, 5 RTS output, 6 DTR output, 7 ground, 20 DSR input. Required flow-control configuration: **UNVERIFIED**.
- ASTM E1381-91 framing and E1394-91 records. Analyzer sends ENQ; host acknowledges frames; analyzer ends with EOT. Result direction is one-way; ACK/NAK reception is required, not order downloading.
- Records H/P/O/R/L. P.4 carries ID; O.4 sample serial number; O.3 and patient name unused. Accession mapping: **UNVERIFIED**. R carries analyte, value, units; first R carries analysis time. No explicit QC discriminator documented.

Frame structure (Service Manual 4.2A, p. 86): STX, one frame-number digit, the record text ending in CR, ETX, two checksum characters, then CR and LF. Each R record holds a sequence number, the analyte code in the universal test ID field, the value and the unit. The manual's example may pad the value with spaces; whether real output does so is **UNVERIFIED**, so keep the value exactly as sent.

Synthetic example written for these notes (made-up values, not taken from the manual; control bytes shown symbolically):

```text
<STX>4R|3|^^^Cl-|104|mmol/L<CR><ETX>74<CR><LF>
```

The header identifies BioCare/Biolyte, firmware and serial number. Confidence: medium-high; manual, not captured bytes. The manual's example is not a replay fixture; only real captured bytes can be one. A printer-text-only parser is inappropriate.

## Sample identification and QC

The V2 reference manual, Table 3-2, pp. 55–58, 63 and 140, gives these entry limits:

- ID: up to 12 letters or digits.
- Sequence number: counted by the analyzer, from 0 to 999.
- Accession: digits only, from 0 to 9999999999.
- Name: up to 20 characters, letters and a few punctuation marks.

Barcode entry supports ID/Acc.; CCD600 by GIGA-TMS is named. Hardware descriptions conflict: specifications say PS/2; service manual describes a separate DB25 male barcode/update port. Actual scanner connector/compatibility: **UNVERIFIED**; do not buy a USB scanner on this evidence.

Printed reports include ID, accession, name, sequence, sample type, date/time and analyte results. This is paper layout, not serial layout. Menu Data Transfer sends stored results through ASTM; automatic live sending is **UNVERIFIED**. QC uses a separate QC workflow. The ordinary Analyze screen on the lab's unit also shows a label that mentions QC runs; that label alone cannot classify a run.

Interpretation: a wrapping sequence counter cannot safely identify a specimen globally. Prefer a lab specimen identifier entered/scanned into ID, after confirming it survives serial transfer. QC-vs-patient output distinction remains **UNVERIFIED**; capture both before enabling import. Name/accession on paper does not prove transmission.

## OpenELIS Analyzer Bridge fit

Read the [README on the develop branch](https://raw.githubusercontent.com/DIGI-UW/openelis-analyzer-bridge/develop/README.md). Confidence: high for documented capabilities on this moving branch, not deployed-version compatibility. It supports serial connections, pinned parsing profiles, ASTM frame acknowledgements and normalized FHIR delivery to OpenELIS. It also supports profile-owned control recognition.

Inference: try a serial ASTM profile first, verifying E1381-91 compatibility with the chosen Bridge release. Map the actual P.4/O.4 identifiers and Na+/K+/Cl- codes explicitly. A line-based text profile would lose the required ASTM handshake. A small custom parser is only a fallback if real records exceed profile capabilities; no evidence yet justifies one. A Biolyte-specific shipped profile was not verified.

The Bridge is designed to deliver to OpenELIS. Reusing it with another web app also requires an agreed receiving contract/adapter; it is not a generic plug-and-play HTTP relay. That work is outside this research.

## Decision and questions for the lab

Keep manual entry of the three values at present: low priority, with unresolved identity/QC mapping and an additional on-site service to operate. The documented ASTM interface makes later connection plausible; it should not require reverse-engineering a printer dump.

Obtain, without patient information leaving the lab:

1. Back-panel and rating-label photos, software version, installed electrodes, and the unit's manuals (especially communication appendix).
2. De-identified patient and QC slips, including flagged/failed results; whether staff actually enter ID or accession today.
3. A local serial capture of patient, QC and retransferred results using synthetic identifiers; compare every field, units, errors and leading zeros against paper.
4. Confirmation of upload settings, cable pinout/handshake, automatic versus menu transfer, and a reliable transmitted QC marker. If QC cannot be distinguished, do not auto-import.
5. Daily electrolyte volume and ID-entry effort: would connecting save meaningful work over typing Na/K/Cl?
