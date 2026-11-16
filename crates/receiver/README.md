# receiver

**What it does:** Provides a reference lab-app endpoint for integration tests and examples. It is a separate runnable test stand-in.

**Where it sits:** Receives signed app-contract deliveries from the bridge and serves order changes back.

**What it must never do:** Make clinical decisions or claim receipt before durable storage.

**Main parts:**

- [lib.rs](src/lib.rs): Durable delivery receipt and order serving.
- [main.rs](src/main.rs): Local reference server; remote use requires a secure front end.
