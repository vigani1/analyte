# LAW status notifications

Research date: 2026-11-16. Later note: ADR 0016, called proposed below, was
accepted on 2026-11-16 and is implemented. Evidence: documented. This is research and a
recommendation, not implemented behavior or evidence of machine qualification.
The proposed decision is [ADR 0016](../../adr/0016-awos-status-notifications.md).
All descriptions below are in our own words.

## What the profile establishes

The controlling sources are IHE PaLM Technical Framework Rev11, Final Text,
2024-04-08: [Volume 1](https://www.ihe.net/uploadedFiles/Documents/PaLM/IHE_PaLM_TF_Vol1.pdf)
and [Volume 2b](https://www.ihe.net/uploadedFiles/Documents/PaLM/IHE_PaLM_TF_Vol2b.pdf).
The actual revision read was the owner's local, gitignored text extraction
of each document in the main checkout's `references/text/`; public URLs may
eventually serve another revision. Neither extraction is included here.

- TF-1 section 5.4.1, Concepts: the manager retains the relation between an
  AWOS and its parent work order. An AWOS identifies a test or panel on one
  analyzer and specimen/container. A specimen or test code alone is not its
  identity. The profile does not specify the safe-storage implementation.
- TF-1 sections 5.4.2.1.1 and 5.4.2.1.2, AWOS Broadcast: a started notice is
  optional for the analyzer. Results can arrive over several notifications.
  Cancellation is a separate exchange whose success the analyzer decides;
  requesting it does not prove it happened.
- TF-2b sections 3.29.1, 3.29.2 and 3.29.4.1.2.1: LAB-29 carries AWOS status
  as well as observations. ORC-1 identifies a status change; ORC-5 carries the
  AWOS status. SC and CA omit RESULT groups; IP may omit them when nothing is
  available yet. An estimate at the start of a run does not require a result.
- TF-2b Appendix C.6, OBR, identifies the AWOS through OBR-2 and the service
  through OBR-4. Appendix C.8, ORC, Table C.8-5 and Figure C.8-1 distinguish
  scheduled, processing, completed and cancelled work. The cancellation path
  in that diagram is from scheduled work, not a general permission to cancel
  any processing work. Appendix C.7 describes observation status separately.
- TF-2b section 2.2.4's enhanced content includes ORC-27, Filler Expected
  Availability Date/Time. C.8, Table C.8-9, limits its representation to a date
  or date/time, with no separate zone or precision component; its zone follows
  MSH-7. Preserve the source spelling and the evidence for any derived offset.
  It is not an actual completion time or a command deadline.
- TF-2b Appendix B.7.6 requires message-level AR with located errors for
  inconsistent returned work, including an unknown manager AWOS. There is no
  per-AWOS partial acceptance in this acknowledgment.

The figure and the status-only message shape answer different questions:
being able to encode CA does not make every preceding-state-to-CA transition
valid. In particular, a LAB-29 CA is not the correlated LAB-28 response to a
manager cancellation request.

## Public implementation and testing evidence

| Source and section | Finding | What it does not establish |
| --- | --- | --- |
| [Siemens Atellica Solution LIS Interface Guide](https://doclib.siemens-healthineers.com/rest/v1/view?document-id=1086358), 11312743 Rev12, 2024-11, About LAW Transactions, p.138; ORC Order Control and Order Status, pp.157-159 | Identifies LIS or middleware as manager. Distinguishes status updates from command responses: IP can have no results; CR and UC answer a requested cancellation. Documents a local operator event for an incoming cancel. | The downstream LIS policy of a separate middleware product; actual emission frequency of empty IP notices. This is a vendor implementation description, not an independently verified integration statement. |
| [Gazelle Order Manager user manual](https://connectathon.ihe-catalyst.net/gazelle-documentation/Order-Manager/user.html), Analytical Work Order Step; Laboratory test results management | Keeps browsable order/specimen/container entities, supports new/cancelled work exchanges and LAB-29 testing. Existing work can be selected for subsequent observations. | Production retention guarantees, an operator UI policy for every empty status, or forwarding all notices to an LIS. |
| [IHE LAW development notes](https://wiki.ihe.net/index.php/Laboratory_Analytical_Workflow), Testing Capabilities and Use Case Support | Separates message validation, simulators, peer configuration and workflow scenarios in its testing strategy. These are historical development/Connectathon planning notes. | Passing a validator proves neither restart custody nor safe clinical handling. These notes do not supersede Rev11. |
| [Roche cobas infinity central lab User Guide](https://elabdoc-prod.roche.com/eLD/api/downloads/52bf3f1e-a08e-ea11-fc90-005056a71a5d?countryIsoCode=ch), software 3.01, publication 4.3, Result entry and validation; Order traces | Public search indexing identifies separate test/validation/host-send status and order trace facilities. | Direct retrieval returned HTTP 401; the full text and LAW-to-LIS mapping were not verified. Do not use this as protocol authority or proof of handling SC/IP/CA. |
| [Roche Viewing Order Traces quick reference](https://diagnostics.roche.com/content/dam/diagnostics/us/en/resource-center/data-bytes-viewing-order-traces-qrg.pdf), introduction and Order traces navigation | Describes an order audit trail used to inspect status, analyzer passage and errors. This is direct vendor evidence for retaining and displaying operational history. | LAW-specific SC/IP/CA ingestion, durability before ACK or forwarding policy. |
| [IICC Supported Instruments](https://ivdconnectivity.org/supported-instruments/), vendor-provided list, last update 2022-07-31 | Lists LAW claims, including partial support for MAESTRIA middleware. | A dated support list is not a detailed integration statement, current qualification or evidence of any status-to-LIS mapping. |

Public searching for LAW vendor integration statements did not yield a
verified statement specifying storage, display and LIS forwarding of these
three empty notifications. Do not infer that policy from a generic claim of
LAW support. [IHE Europe's FAQ](https://www.ihe-europe.net/about-us/faq),
conformance and Connectathon sections, distinguishes product declarations,
peer testing and conformity assessment. Qualification must name the version,
actor, options and scenarios actually tested.

## HL7 practice and the LIS boundary

[HL7 Australia, Observation Reporting](https://hl7.org.au/archive/hl7v2wg/4-Observation-Reporting_1278278.html),
sections 4.4.1.22 and 4.4.1.25, distinguishes order status in ORC-5,
request/report status in OBR-25 and individual observation status in OBX-11.
Its generic reporting rules include OBR precedence where both levels apply.
That is useful evidence for preserving scope; it is not a substitute for
LAW's specific field usages. Never infer a LAW result status from ORC-5 or
apply an OBR-25 mapping to a field that LAW does not use that way.

The evidence supports retaining operational status separately from clinical
observations. It does **not** establish a universal middleware convention to
forward every SC/IP/CA to the LIS, a common public storage schema, or a common
CA reconciliation policy. Displaying work status and transmitting reportable
results are distinct concerns. How a particular middleware connects them
requires its interface specification and a recorded exchange.

## Recommendation for Analyte

Keep an append-only interpretation of each status receipt linked to the
retained manager AWOS and exact order item. Project scheduled/in-progress/
analyzer-cancelled status on the protected admin page; normal progress should
not create a parked item. Keep command acceptance and cancellation response
states separate. No observation, result identity, result revision or result
delivery is created for an empty notice.

Initially leave contract v1 unchanged and forward no new app event for these
notices. Its order-event state enum has neither in-progress nor analyzer-
cancelled semantics. An optional field cannot make an unknown enum safe for
old apps. A future interoperable AWOS notification needs an explicit contract
decision, capability agreement and its own stable delivery identity. Until
then the admin-only limitation must be part of LAW deployment sign-off.

Before enabling a LAW machine, test empty progress/estimates, independent
analyzer cancellation, requested-cancel races, retransmission, reordered
notifications, unknown AWOS, and crash boundaries. Neither this research nor
the proposed ADR closes the existing LAW conformance gate.
