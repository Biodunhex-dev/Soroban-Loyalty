## Pull Request Checklist

Before submitting your PR, please review the following checklist:

- [ ] I have run tests locally and they all pass.
- [ ] My code follows the project's coding style and guidelines.
- [ ] I have added appropriate tests for new features or bug fixes.
- [ ] **Smart Contract Changes**: If I modified anything under `contracts/`, I updated `docs/audits/smart-contract-audit-status.yml` to reflect whether a re-audit is now required.
- [ ] **Database Schema Changes**: If I altered `schema.sql`, I have:
  - [ ] Updated the ERD (`docs/erd.png`).
  - [ ] Updated the schema documentation in `docs/database.md`.
- [ ] Runbook / Operations documentation is updated if necessary.
- [ ] **Changelog & Versioning**:
  - [ ] I have updated the `CHANGELOG.md` following the [Keep a Changelog](https://keepachangelog.com/) format.
  - [ ] My PR title follows [Conventional Commits](https://www.conventionalcommits.org/) (e.g., `feat:`, `fix:`, `BREAKING CHANGE:`).
