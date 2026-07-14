# Repository review settings

GitHub repository administrators should configure `main` with a ruleset that:

1. requires pull requests and all required CI/WASM checks;
2. requires at least one approval for changes matched by `CODEOWNERS`;
3. dismisses stale approvals when security, plugin ABI, model trust, importer, or
   workflow files change;
4. requires conversation resolution and blocks force-push/deletion of `main`;
5. permits an audited maintainer bypass for urgent recovery without requiring a
   second review for documentation-only or mechanical dependency changes.

The repository files can request review but cannot enable GitHub branch protection.
Administrators must verify the live ruleset after merging this document. The CI
large-diff job emits a non-blocking warning above 80 files or 5,000 changed lines so
reviewers explicitly inspect security, ABI, model, and platform impact.
