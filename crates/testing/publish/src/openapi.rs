//! Builds the OpenAPI 3.1 document for the three contract v1 calls; its bodies refer to the contract schema file (ADR 0033).
//!
//! The calls are the ones a lab app answers: `POST /results`,
//! `POST /order-events` and `GET /orders`. Each status code says what Analyte
//! does with the answer, as docs/contract/v1.md does.
use serde_json::{Value, json};

/// The contract schema file the bodies refer to, beside this document.
pub(crate) const SCHEMA_FILE: &str = "contract-v1.schema.json";

/// A reference to one definition of the contract schema file.
fn body(name: &str) -> Value {
    json!({ "$ref": format!("{SCHEMA_FILE}#/$defs/{name}") })
}

/// A JSON answer with the definition `name`.
fn answer(description: &str, name: &str) -> Value {
    json!({
        "description": description,
        "content": { "application/json": { "schema": body(name) } },
    })
}

/// The answers of a delivery call (`POST /results` or `POST /order-events`).
fn delivery_answers() -> Value {
    json!({
        "200": answer("Stored durably before answering. `accepted` for a new delivery ID, `duplicate` for a repeat with the same bytes. Analyte marks the delivery done.", "StatusBody"),
        "400": { "$ref": "#/components/responses/Refused" },
        "401": { "$ref": "#/components/responses/Unauthorized" },
        "403": { "$ref": "#/components/responses/Unauthorized" },
        "404": { "description": "Wrong destination URL. Analyte pauses the destination for setup repair; it never follows a redirect (3xx is treated the same way)." },
        "408": { "$ref": "#/components/responses/RetryLater" },
        "409": { "description": "The delivery ID was already stored with different bytes or under the other endpoint. Analyte parks the delivery for an operator." },
        "413": { "description": "The app's body limit is below the contract's 1,048,576-byte document limit. Analyte pauses the destination for setup repair." },
        "422": { "$ref": "#/components/responses/Refused" },
        "429": { "$ref": "#/components/responses/RetryLater" },
        "5XX": { "$ref": "#/components/responses/RetryLater" },
        "default": { "description": "Any other status: Analyte parks the delivery for an operator." },
    })
}

/// A signed delivery call with `request` as its body.
fn delivery(summary: &str, description: &str, request: &str) -> Value {
    json!({
        "summary": summary,
        "description": description,
        "parameters": [
            { "$ref": "#/components/parameters/WebhookId" },
            { "$ref": "#/components/parameters/WebhookTimestamp" },
            { "$ref": "#/components/parameters/WebhookSignature" },
        ],
        "requestBody": {
            "required": true,
            "content": { "application/json": { "schema": body(request) } },
        },
        "responses": delivery_answers(),
    })
}

/// The OpenAPI 3.1 document, as `docs/contract/openapi.json` holds it.
#[must_use]
pub(crate) fn document() -> Value {
    json!({
        "openapi": "3.1.1",
        "jsonSchemaDialect": "https://json-schema.org/draft/2020-12/schema",
        "info": {
            "title": "Analyte app contract",
            "version": "1",
            "summary": "The calls a lab app answers for Analyte: result deliveries, order events and the order feed.",
            "description": "Contract version 1 (docs/contract/v1.md, ADR 0008). Analyte calls the lab app; the lab app never calls Analyte. Bodies are UTF-8 JSON of at most 1,048,576 bytes; unknown fields are ignored. Body schemas are in contract-v1.schema.json beside this file. Generated from the Rust contract types; do not edit by hand.",
            "license": { "name": "Apache-2.0", "identifier": "Apache-2.0" },
        },
        "servers": [{
            "url": "{base}",
            "description": "The lab app's base URL, as set up in Analyte; the three paths are under it.",
            "variables": { "base": { "default": "https://lab-app.example/analyte" } },
        }],
        "security": [{ "bearer": [] }],
        "paths": paths(),
        "components": components(),
    })
}

/// The three calls.
fn paths() -> Value {
    json!({
            "/results": { "post": delivery(
                "Deliver results",
                "One delivery of results read from one machine message. Store the delivery ID with its effects before answering 200, and answer a repeat of the same ID and bytes as duplicate. Key your test table on source.model and each part's test code.",
                "Delivery",
            ) },
            "/order-events": { "post": delivery(
                "Report an order's progress",
                "One change of an order's state on a machine. An uncertain state means the outcome is unknown; it never asks for a resend.",
                "OrderEvent",
            ) },
            "/orders": { "get": {
                "summary": "Read the order feed",
                "description": "Without a cursor: a paged full snapshot of the latest version of each order. With the last cursor: the changes since, including tombstones. Commit each page with its cursor; complete is true on the last page.",
                "parameters": [
                    {
                        "name": "cursor",
                        "in": "query",
                        "required": false,
                        "description": "The next_cursor of the previous page or poll; opaque, at most 256 bytes.",
                        "schema": body("Cursor"),
                    },
                    {
                        "name": "limit",
                        "in": "query",
                        "required": false,
                        "description": "Orders per page.",
                        "schema": { "type": "integer", "minimum": 1, "maximum": 100, "default": 100 },
                    },
                ],
                "responses": {
                    "200": answer("One page of orders.", "OrdersPage"),
                    "400": answer("A limit outside 1 to 100 or a cursor over 256 bytes.", "ErrorBody"),
                    "401": { "$ref": "#/components/responses/Unauthorized" },
                    "403": { "$ref": "#/components/responses/Unauthorized" },
                    "410": answer("The cursor is unknown or expired (one hour after it was made). Analyte reloads a complete snapshot.", "ErrorBody"),
                    "429": { "$ref": "#/components/responses/RetryLater" },
                    "5XX": { "$ref": "#/components/responses/RetryLater" },
                },
            } },
    })
}

/// The security scheme, the signature headers and the shared answers.
fn components() -> Value {
    json!({
            "securitySchemes": {
                "bearer": {
                    "type": "http",
                    "scheme": "bearer",
                    "description": "The destination's API key: 1 to 4096 printable ASCII characters (space to ~), on every request.",
                },
            },
            "parameters": {
                "WebhookId": {
                    "name": "webhook-id",
                    "in": "header",
                    "required": true,
                    "description": "Standard Webhooks 1.0.0 message ID: the body's delivery_id, the same on every retry.",
                    "schema": body("Uuid7"),
                },
                "WebhookTimestamp": {
                    "name": "webhook-timestamp",
                    "in": "header",
                    "required": true,
                    "description": "Standard Webhooks 1.0.0 send time in integer Unix seconds. Refuse one more than 300 seconds from your clock in either direction.",
                    "schema": { "type": "string", "pattern": "^[0-9]+$" },
                },
                "WebhookSignature": {
                    "name": "webhook-signature",
                    "in": "header",
                    "required": true,
                    "description": "Standard Webhooks 1.0.0 signatures: space-separated v1,<base64 HMAC-SHA256> candidates over the exact bytes of id.timestamp.body. Accept any candidate that matches a trusted whsec_ secret, comparing in constant time.",
                    "schema": { "type": "string", "pattern": "^v1,[A-Za-z0-9+/]+={0,2}( v1,[A-Za-z0-9+/]+={0,2})*$" },
                },
            },
            "headers": {
                "RetryAfter": {
                    "description": "Optional: delay seconds or an IMF-fixdate HTTP date. Analyte waits at most 60 seconds; a malformed value or one over 64 characters is ignored.",
                    "schema": { "type": "string", "maxLength": 64 },
                },
            },
            "responses": {
                "Refused": answer("The app refuses this document. Analyte parks the delivery with the app's detail for an operator; nothing is lost.", "ErrorBody"),
                "Unauthorized": { "description": "Wrong or missing key, signature or timestamp. Analyte pauses the destination for setup repair." },
                "RetryLater": {
                    "description": "Busy or unavailable. Analyte retries the same ID and bytes with capped backoff.",
                    "headers": { "Retry-After": { "$ref": "#/components/headers/RetryAfter" } },
                },
            },
    })
}
