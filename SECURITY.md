# Security

Prism Hub installs programs, so it is built to refuse anything it cannot verify:

- It reads only a catalog signed with the hub's own key (separate from the apps' update keys).
- It downloads only over HTTPS from github.com and GitHub's release download host, following
  each redirect by hand and checking every hop.
- Every installer must match the catalog's SHA-256 and carry a valid, timestamped Windows code
  signature from the expected signer. A file that fails is deleted before anything runs.
- It never closes a running app to update it, and never runs anything as administrator.
- Update checks send nothing beyond an ordinary web request.

## Reporting a problem

Please report security problems privately through GitHub's "Report a vulnerability" button on
this repository's Security tab rather than in a public issue.

## Keys

The catalog key's private half and its password are kept off GitHub by the maintainer. If it is
ever lost or exposed, a new hub release carrying a new public key is needed, installed by hand.
