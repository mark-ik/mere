# mere-eidetic

Package `mere-eidetic`; the library is `eidetic`, so consumers write
`use eidetic::…`.

The owner-scoped local memory lane for the [mere](https://crates.io/crates/mere)
browser. It owns the durable typed vocabulary: blob manifests, typed payloads,
schemas, engrams, bundles, packs, encrypt-at-rest markers, browsing traces, and
content-addressed model artifacts. Storage-backend-agnostic and host-agnostic.

## Modules

| Module | Contents |
|---|---|
| (root) | `Request` / `Response` (blob load/save), `Error`, `Result`, `dispatch`, `bootstrap` |
| `schema` | `Hash` (multihash-aware, `HashFn::Blake3` today), `HashError`, `ManifestId`, `SchemaRef`, `Timestamp`, `PrivacyClass`, `ProvenanceOrigin`, `ProvenanceRecord`, `TrustLevel`, `TrustEnvelope`, `SignatureRef`, `ModerationState` |
| `manifest` | `BlobManifest`, `BlobSource` (`Local`, `Embedded`, `Https`, `Iroh`, `LocalFile`, `LocalOnlyRef`), `BlobFetcher`, `NoFetcher`, `save_manifest`, `load_manifest`, `list_manifests`, `delete_manifest`, `resolve_blob` |
| `typed` | `TypedPayload`, `save_typed`, `load_typed`, `list_typed`, `save_typed_sealed`, `load_typed_sealed` |
| `schema_def` | `SchemaDefinition`, `SchemaFormat`, `SchemaValidator`, `MereNativeSchemaBuilder`, `MereNativeSchemaBody`, `MereNativeFieldSpec`, `MereNativeValidator`, `JsonSchemaValidator`, `JsonLdValidator`, `bootstrap_meta_schema`, `save_schema`, `load_schema`, `find_schema_by_id`, `validate_payload`, `validate_against_schema` |
| `engram` | `Engram`, `TimeBounds` |
| `bundle` | `Bundle`, `BundleMember`, `save_bundle`, `load_bundle`, `verify_required_members` |
| `pack` | `PackManifest`, `PackPart`, `PackPartRole`, `PackVerdict`, `canonical_bytes`, `sign_pack`, `verify_pack` |
| `seal` | `PayloadSealer`, `SealEpochId`, `SealedBlobRef`, `seal_marker`, `seal_payload_for_store`, `resolve_sealed_blob`, `is_private_lane` |
| `browsing` | `BrowsingTrace`, `BrowsingMemory`, `TraceEvent`, `TraceTransition`, `PageRef`, `save_trace`, `bootstrap_browsing_schema`; `browsing::lineage::project_lineage` behind the `lineage` feature |
| `deleted` | `DeletedNode`, `record_deleted`, `list_deleted`, `purge_deleted`, `clear_deleted` |
| `models` | `ModelManifest`, `ModelLibrary`, `ModelComponents`, `OpaqueBlob` |

## The store seam

`eidetic::Store` is a re-export alias for [`muniment::Backend`]. `Backend`,
`MemoryBackend`, and `WriteOp` are re-exported alongside it, and
`muniment::error::StoreError` converts into `eidetic::Error` through a `From`
impl. Any muniment backend is an eidetic store.

The trait is `async` and `?Send`: a browser store awaits JS promises, native
stores return ready futures.

## Features

| Feature | Default | Effect |
|---|---|---|
| `pack-signing` | yes | Pack signing and verification via `personae` (`dep:identity`) |
| `json-schema` | yes | Full JSON Schema validation via `jsonschema`. Without it, stored JSON Schema definitions return a "validator unavailable" error |
| `lineage` | no | `browsing::lineage`, which projects a `chartulary::stemma` into `BrowsingTrace` engrams (`dep:chartulary`) |

## Dependencies

`muniment` (path), `async-trait`, `blake3`, `serde`, `serde_json`. Optional:
`personae` (as `identity`), `jsonschema`, `chartulary`.

## Companions

- `mere-eidetic-fjall`: `FjallStore`, the production-default native backend.
- `mere-eidetic-https-fetcher`, `mere-eidetic-iroh-fetcher`: `BlobFetcher`
  implementations for non-local `BlobSource` variants.
- `mere-eidetic-search`: a tantivy `TrailIndex` minted from `BrowsingTrace`
  engrams.

## License

MPL-2.0 (see LICENSE).

[`muniment::Backend`]: https://docs.rs/muniment/latest/muniment/trait.Backend.html
