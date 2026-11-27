# 0038. Optional Needs review notices to the lab app; the decision stays in Analyte

Status: accepted, 2026-11-27 (owner: "we can support that as well"). Extends
ADRs 0003, 0008 and 0024.

## Context

When Analyte holds something from a machine for an operator (a result without
a trustworthy identity, a held repeat, a delivery the lab app refused, a
message it could not read), the lab app is never told. Lab staff work in the
lab app, so they only see the item if they also open Analyte's pages. A lab
app that knows "sample 000Ab12 waits for review in Analyte" can show it next
to the sample it already holds.

## Decision

1. **The decision stays in Analyte.** A notice only says that an item waits
   for review in Analyte, and later that staff settled it there and how. The
   lab app shows it; it never answers it, never settles the item and never
   decides anything from it. No contract call lets a lab app resend, rerun,
   supersede or dismiss. This keeps one place for each clinical decision
   (ADR 0003): the item, its evidence and its audit stay in Analyte.
2. **What a notice carries.** `POST /review-notices` names the machine and its
   model (`source`, as results do), a stable item reference (`item`, the
   Needs review item's number), and either `opened` (a reason from a closed
   list, the sample ID exactly as received when the item concerns exactly one
   sample, the machine's own test codes, when it was parked) or `closed` (how
   staff settled it and when). The lab app owns patient context, so the sample
   ID is allowed. A notice never carries a raw message, a value or the stored
   reason text, which may quote a value. Analyte's pages are served only on
   the Analyte computer (loopback), so a link a lab app user could open does
   not exist; staff find the item by its number. A link field can be added
   when one exists; unknown fields are ignored, so that is additive.
3. **Opt-in per lab app.** `review_notices = true` in a lab app's settings,
   "Send Needs review notices: yes / no" on Setup → Lab apps and
   `analyte lab-apps set <name> --review-notices yes|no`, through the one save
   path. Off by default and left out of the snapshot while off, so existing
   snapshot bytes and digests do not change. A lab app is told about items
   parked after its setting takes effect at engine start; items already open
   are not replayed.
4. **Same delivery rules as results.** Each notice is frozen once in the
   outbox with its own delivery ID (the notice ID), signed and retried with
   the same ID and bytes like any delivery (ADR 0008). Each item has at most
   one opening and one closing notice per lab app; the closing one is queued
   only after the opening one was accepted, and waits behind it. Notices about
   a notice are never made.
5. **A lab app that takes none.** 404 or 501 from `/review-notices` means the
   lab app does not take notices. That notice and every one still waiting are
   marked declined, a finished state that is never counted as waiting; no more
   are queued; nothing is parked or paused, and results go on. The lab app
   page says so with the time and reason. Analyte asks again after its next
   start while the setting is on. Every other answer means what it means for
   results: 401/403, 3xx and 413 pause the lab app, 400/422 and 409 park the
   notice for an operator, 408/429/5xx retry.
6. **Bounds and keeping.** A notice is at most 256 test codes and the contract
   document limit. Data keeping never purges a message that has a Needs
   review item, resolved or not (ADR 0013), so a notice is kept exactly as
   long as the item and message it names; nothing new is retained.

## Consequences

- The outbox gains a third kind (`review_notice`) and a seventh state
  (`declined`, only for notices). Store schema step 20 to 21 rebuilds the
  outbox table with every row, attempt link, index and trigger kept.
- An item Analyte cannot trace to a machine (the lab app's own order feed, an
  item whose machine was removed) gets no notice; it is still in Needs review.
- Notices count as waiting deliveries until accepted or declined, so a lab app
  that is down shows them with its results.
