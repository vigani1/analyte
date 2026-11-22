# publish

**Kind: testing.** Writes the machine-readable files published for lab-app
developers.

**What it does:** Builds four files in `docs/contract/` from one source each:
the contract v1 JSON Schema (`contract-v1.schema.json`) and the OpenAPI 3.1
document of the three calls (`openapi.json`) from the Rust contract types,
and the machine catalog (`machines.json`, with `machines.schema.json`) from
the `machines/` folder ([ADR 0032](../../../docs/adr/0032-machine-model-and-published-schema.md)).
`cargo run -p analyte-publish` rewrites them; `-- --check` only reports the
stale ones. CI and the release workflow check that regenerating changes
nothing, and each release attaches the four files.

**Where it sits:** Beside the contract checker. It reads the contract types
and the model folders; Analyte itself never runs it.

**What it must never do:** Guess a catalog fact a model folder does not state
(a blocked definition has no protocol or code systems), or let a file drift
from the types it describes.

**Main parts:**

- `lib`: The four files, with sorted keys so their bytes never depend on the
  build.
- `catalog`: One entry per `machines/<vendor>/<model>/` folder.
- `openapi`: The three calls, their headers and what Analyte does with each
  answer.
- `main`: The `analyte-publish` command.

The tests check every published contract vector against the schema.
