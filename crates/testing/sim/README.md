# sim

**Kind: core.** Groups files by the data and operations they handle.

**What it does:** Plays the machine side of synthetic recordings and compares the bridge replies byte for byte. It is a test building block.

**Where it sits:** Receives recordings and a test connection; sends recorded machine events and checks host output.

**What it must never do:** Act as a production analyzer, make clinical decisions or treat synthetic tests as real-machine proof.

**Main parts:**

- `lib`: Recorded conversation replay and exact-byte comparison.
- `recordings`: Synthetic conversations, including failure cases.

[Technical details and safety guidance](DETAILS.md).
