# Code Signing Policy

YTAudioBar Windows releases are signed through [SignPath Foundation](https://signpath.org/),
a free code-signing service for open-source projects.

> Signed with [SignPath.io](https://signpath.io), certificate by [SignPath Foundation](https://signpath.org)

## Team Roles

| Role          | Responsibilities                                                      |
| ------------- | --------------------------------------------------------------------- |
| **Authors**   | Trusted committers who may merge code directly to `main`              |
| **Reviewers** | Review pull requests from external contributors                       |
| **Approvers** | Approve each signing request before a release is signed and published |

Current team members and their roles are managed through the repository's GitHub team settings.

## Signing Process

1. A release build is triggered by pushing a `v*.*.*` tag to `main`
2. GitHub Actions builds the Windows installer (`.exe`) on `windows-latest`
3. The unsigned installer is submitted to SignPath Foundation for signing
4. An Approver reviews and approves the signing request in the SignPath dashboard
5. SignPath returns the signed installer, which is packaged and uploaded to GitHub Releases

## Scope

The signing certificate is used exclusively to sign YTAudioBar release binaries built
from this repository. It is never used to sign third-party software.

## Privacy

Build metadata (source commit SHA, artifact hash, build timestamp) is transmitted to
SignPath.io as part of the signing request. No user data or application data is shared.
SignPath's privacy policy applies: https://signpath.io/privacy-policy

## License

YTAudioBar is released under the [MIT License](LICENSE).
