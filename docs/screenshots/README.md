# Screenshot provenance

This directory contains two kinds of images. Do not describe every image as a synthetic preview.

## Synthetic or empty preview images

These are the four images used in the root `README.md`:

| File | Provenance |
| --- | --- |
| `cleanup-light.png` | Empty software-checkup state; no scan rows and no machine data. |
| `events-light.png` | Sample rows (`browser.exe`, `notepad.exe`) and sample timestamps; synthetic data. |
| `posture-dark.png` | Sample counts and signature version `1.2.3.4`; synthetic data. |
| `date-picker-light.png` | Same synthetic event rows as `events-light.png`, with the date picker open. |

## Historical validation captures

The following images are read-only UI validation captures from the Windows 11 test machine used for the release checks. They are **not** synthetic marketing images:

- `home.png`
- `events.png`
- `events-analysis.png`
- `native-protection.png`
- `services-tasks.png`
- `scheduled-tasks.png`
- `asset-associations.png`
- `security-posture.png`

They show Windows system component names, service/task names, counts, PIDs, timestamps, and local system paths. No user-profile account name, host name, personal software inventory list, or credential is intentionally shown. They are referenced by the technical validation notes and ADRs, not by the root README preview section.

If a future screenshot contains real user software names or personal data, replace it with a synthetic capture or remove it from the public tree. Do not treat this provenance file as a substitute for inspecting new images before publication.
