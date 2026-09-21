# Curated ECC resources

Arena packages a bounded allowlist of templates and skills from
`https://github.com/affaan-m/ECC` as reference resources. This is not the ECC
runtime and it does not modify any user-level Claude, Codex, or OpenCode
configuration.

- Upstream commit: `934195f955cf0da847d59fcd6f68856bce112d8b`
- Upstream license: MIT (see the upstream repository's `LICENSE`)
- Catalog: `catalog.json`

Imported text is treated as untrusted procedural content. Arena selects only
allowlisted IDs, binds them to a bounded tool profile, and keeps Product OS
authority, verification, and Apply decisions outside the imported material.
