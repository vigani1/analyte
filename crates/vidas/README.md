# analyte-vidas

Evidence level: **documented**. The implementation was built from
[research notes](../../docs/research/vidas.md), whose sources were BCI Link
161150-486 revisions A/B and VIDAS PC User Manual F. Synthetic property tests
are not simulator evidence.

Pure vendor framing, tagged syntax and one-transfer conversations; no I/O or
clock. Only the VIDAS PC Literal family is supported, not ASTM, XML/FTP, mini
VIDAS or VIDAS 3 extensions. Work lists are one validated request per session;
multiple-packet download sessions are deliberately outside this API.

## Required installed settings

There are no configuration defaults. Supply every framing setting explicitly:

- Literal versus Alternate ACK boundary.
- Maximum RS chunk length (1–80) and payload length (1–1920). Documented ceilings
  are enforced; installed chunking remains an explicit choice.
- Checksum span (`include_stx`, normally false according to the notes), outgoing
  hex letter case and whether incoming case is flexible.
- Control CR/LF suffix: none, CR, LF or CRLF. This API applies the chosen suffix
  uniformly to controls; a site with selectively suffixed controls must not use
  this profile. Whether suffix bytes participate in the checksum is required
  separately (`include_suffix`). STX and its suffix are excluded together unless
  `include_stx` is selected. ETX is always outside the sum.
- Sample ID limit: exactly the installed 12 or 17 character policy. Printable
  ASCII is preserved, never trimmed, folded or truncated. Additional installed
  punctuation restrictions belong to the host's order policy.
- ACK, interbyte, whole-session and durable-commit deadlines; site retry count
  and backoff. Retry settings record site policy, but never trigger a resend.
  Timeout, collision, NAK, disconnect and unexpected input stop and park. No
  undocumented abort sequence is emitted. Reconcile before a new conversation.

The checksum span/case, suffix treatment, chunking, timers and retry behavior
must be confirmed against installed settings even where the notes state a
vendor rule. There are no imported ASTM timing constants.

## Host contract and unresolved research

Journal **all** transport bytes before decoding, including rejected/oversize
input and ETX/EOT after Literal's commit boundary. Keep the protected journal,
driver version and effective configuration together. Decoder errors stop that
instance. Feed one byte at a time and consume each emitted packet immediately;
read splitting and merging therefore do not change boundaries.

Reserve storage capacity before admitting upload ENQ. `Store` is a request;
only a matching durable commit may produce `Stored`. A commit failure never
emits packet ACK. Normal Literal then waits for ETX and EOT; Alternate already
consumed ETX. Preserve incomplete durable receipts across disconnect/restart;
never infer that timeout means the peer received nothing. Host owns bounded
shutdown, timer generations and suffix validation of control events. Cancel
stale timers and enforce the whole-session timer independently of progress.

Unknown tags survive. Required fields, known-tag order, documented field length
limits and direction are checked. Dates, calls and dilution remain exact text;
no locale is guessed. `qn` is mapped to a model text value, `ql` to an exact call,
`nc` to a source flag and `y3` to a separate original unit. This intentionally
avoids the notes' ql/qn contradiction, comma/dot differences, omitted units and
integer/ratio dilution ambiguity. No numeric conversion or dilution multiplication
occurs. Dates, operator, serial and instrument identifiers remain in the message.
No section/slot is invented. Sample ID alone is never a result identity.

Other UNVERIFIED research points remain outside this pure crate: software/BCI
version and licensing/activation, full cable pinout/genders/modem wiring, flow
control and COM settings, handheld-ID workflow, patient versus QC classification,
location extensions, ID reuse/repeats/components, enabled assay codes and language,
release/rejected-order workflows, installed assay revisions, and XML/FTP
availability/schema/paths/retention. Host configuration must provide confirmed
transport and assay mappings; this crate ships no assay catalog or guessed codes.
Missing identity/classification/mapping evidence requires parking under ADR 0003.
The interpretation of `valid` is never clinical acceptance. Unknown order codes
can be rejected by the PC after transport ACK; `Complete` promises transport only.

Synthetic automated checks do not establish interoperability with an installed
VIDAS PC. No hardware recording or official simulator qualifies this capability;
evidence remains documented and site validation is still required.


## Program receive integration

The host retains raw receipts and parks unresolved identities; see
[program receive behavior](../analyte/README.md).

## Program work-list download

The runtime uses the shared reservation and recovery rules in
[program orders](../analyte/ORDERS.md). Protocol limits follow below.

The host accepts at most 100 assays per app order (also subject to configured
bounds), one explicit 1-4 byte assay code per request. Patient IDs must match the
configured namespace and fit 17 printable ASCII bytes; sample IDs must fit the
installed 12- or 17-byte ceiling. Values retain case and leading zeros; missing
patient context, unmapped tests and over-long IDs are rejected before sending.
Literal has no specimen-type or physical-position tag. Supplied specimen type
is rejected as unmapped; it is never placed in origin `so` or structural `si`.
No name, demographics, position or dilution is invented. Framing, suffix,
checksum and deadline settings remain explicit as listed above. Multi-packet
downloads and machine/application acceptance evidence remain outside this scope.

Program/reference-receiver download tests are in
[`tests/orders/vidas.rs`](../analyte/tests/orders/vidas.rs); the
[coverage table](../analyte/ORDERS.md#vendor-end-to-end-coverage) maps each
required case to its test. These synthetic conversations leave evidence at
**documented**. Raw transport and order evidence stay in protected storage;
ordinary logs contain no patient data.

Synthetic TCP tests in `crates/analyte/tests/vendor_receive.rs` exercise custody,
parking, operator visibility, zero reference-receiver result deliveries and storage
failure. They do not raise the documented evidence level. No current vendor
crate path supplies a resolved result identity for a delivery test.

Conversation state and deadline policy live in `analyte-dialog`, in `src/vidas.rs`.
Conversation tests live in `crates/dialog/tests/vidas.rs`.

Checksum envelopes live in `crates/framing/src/vidas.rs`.
