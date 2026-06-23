# Open Source and Business Model

## Recommended Model

QuickNote is best positioned as an **open-source local-first productivity app with a paid hosted cloud**.

Recommended structure:

- **License:** AGPL-3.0-or-later for the public repository.
- **Commercial option:** offer a separate commercial license for white-label distribution, proprietary forks, enterprise embedding, and managed private deployments.
- **Paid product:** hosted QuickNote Cloud with sync, attachment storage, account management, backups, and team features.

This fits the product because the desktop client is trust-sensitive and benefits from source visibility, while the ongoing business value sits in reliable sync, storage, hosted operations, and support.

## Why AGPL

QuickNote includes a server, web app, sync protocol, and desktop client. A permissive license would maximize adoption, but it would also allow hosted competitors to run modified server versions without contributing improvements back. AGPL keeps the hosted-service surface open: if someone modifies and offers the network service, they must provide the corresponding source.

The tradeoff is that some companies avoid AGPL dependencies. The commercial license exists for those users.

## What Stays Open

- Desktop app and local-first storage.
- Web client.
- Cloud API server.
- Sync protocol and shared contracts.
- Self-hosting Docker Compose setup.
- Core notes, clipboard, attachments, and history workflows.

## Paid Cloud Features

The hosted service can charge for operational value rather than hiding basic functionality:

- Cross-device cloud sync.
- Hosted attachment storage with quotas.
- Daily backups and point-in-time recovery.
- Team spaces and shared notebooks.
- Web access with managed auth.
- Priority support and migration assistance.
- Admin controls for organizations.

## Pricing Shape

A practical first pricing model:

- **Free:** local desktop app, self-hosting, limited hosted trial.
- **Pro:** individual hosted sync and storage.
- **Team:** shared workspaces, admin controls, higher storage, priority support.
- **Enterprise:** SSO, audit requirements, private deployment, commercial license.

## Repository Positioning

The public GitHub repository should make the open-source promise obvious:

- Include `LICENSE`.
- Document self-hosting clearly.
- Keep `.env.example` safe and complete.
- Use issues/discussions for community support.
- Put paid hosted features in documentation as hosted operations, not as a hidden-source fork of core editing.

## Risks

- AGPL can reduce corporate adoption unless commercial licensing is easy to understand.
- Hosted sync must be excellent; otherwise users will self-host or stay local.
- The product needs a strong privacy story because notes and clipboard history are sensitive.

## Recommendation

Use **AGPL-3.0-or-later now**, keep all core code public, and monetize the hosted cloud plus commercial licensing. This leaves the project credible as open source while preserving a path to revenue.
