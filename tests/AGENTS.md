# Test Agent Guide

Use this file when editing `tests/**` or adding repo-level fixtures.

## Fixture Rules

- Add tests to the closest existing module before creating a new test file.
- Keep each fixture minimal and complete. Include `package.json` at minimum.
- Add `tsconfig.json` only when TypeScript resolution, path aliases, project references, or framework conventions require it.
- Fixture files may intentionally contain invalid code, but the test name and fixture path should make that intent clear.
- Do not copy large real project output into fixtures. Use small generic examples.

## Behavior Rules

- Test behavior through public analysis APIs or CLI output, not private implementation details.
- For bug fixes, include a failing minimal repro and at least one real-project smoke when practical.
- Normalize dynamic values before snapshots: elapsed time, absolute paths, versions, and platform separators.
- Prefer path assertions based on `Path::ends_with`, components, or normalized relative strings.

## Validation

- Run the narrow test first.
- If the fixture affects shared pipeline behavior, run the relevant crate integration tests.
- If CLI output changes, update snapshots deliberately and review the diff.
