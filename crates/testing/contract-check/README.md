# contract-check

**Kind: testing.** Checks a lab app against contract v1.

**What it does:** Runs the published contract v1 test vectors against a lab
app's real endpoints and prints PASS, FAIL or SKIP per case. It is a separate
program for app developers in any language.

**Where it sits:** Sends signed deliveries and order polls to the app exactly as
Analyte does, and compares the answers with the vectors in
`docs/contract/vectors/`.

**What it must never do:** Print the API key or signing secret, take them as
command-line arguments, follow redirects, skip certificate checks, or send
anything but synthetic vector data.

**Main parts:**

- `lib`: The run and its report.
- `settings`: App address and credentials from the environment or a file.
- `client`: Bounded, signed HTTP requests to the app.
- `deliveries`: Result and order-event delivery cases.
- `orders`: Order feed, cursor and seeded-order cases.
- `vectors`: The published vectors, embedded so the program is self-contained.
- `main`: The `analyte-contract-check` command.

How to use it: [Check your app](../../../docs/contract/README.md).
