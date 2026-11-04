# CycloneDX schema validation

Date: 2026-11-02; source-only release annotation: 2026-11-04

> This is a source-only release. `SBOM.source.cdx.json` is the current source
> dependency inventory. `SBOM.binary.cdx.json` is retained only as a historical binary
> build composition record; it is not part of this release and does not describe
> downloadable assets.

## Schema and validator

- Specification: CycloneDX 1.5 JSON
- Schema URL: https://cyclonedx.org/schema/bom-1.5.schema.json
- Schema SHA-256: `a00fcba23a44b72179ac1f288be4c3529b59f6e1b3719709a40685f177516b46`
- Referenced schema `spdx.schema.json` SHA-256:
  `33863a360fc4d348e183d89c1ca7aa0877f481e7eb55be2d4102a30386188a10`
- Referenced schema `jsf-0.82.schema.json` SHA-256:
  `2faf5eb3651f2ae5f46091a131770d8d847bbd121139d19c85fc7051bfa58c46`
- Validator: Python `jsonschema` 4.26.0 with JSON Schema draft-07 (`Draft7Validator`)
- Validation mode: local copies of the official schema and its two referenced schemas;
  `$ref` resolution against `http://cyclonedx.org/schema/`.

## Results

| File | Components | Dependencies | Validation errors |
| --- | ---: | ---: | ---: |
| `SBOM.source.cdx.json` | 642 | 643 | 0 |
| `SBOM.binary.cdx.json` | 201 | 198 | 0 |

Both files also pass the dependency-reference closure check: every `dependsOn` target
resolves to a component or the root application reference.

## Reproduce

The validation used a temporary local `jsonschema` install and the three official
schema files above. It did not install or use the CycloneDX CLI. This is validation
against the official schema, not legal review and not a CycloneDX conformance
certification.
