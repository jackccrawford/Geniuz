# Security

## Reporting a problem

If you find a security problem in Geniuz, please write to **jcc@managedv.com** before
making it public. Say what you found, the version (`geniuz --version`), and how to
reproduce it if you can. You will get a reply from a person.

Please don't open a public issue for a problem that could put other people's notes at
risk until a fix is out.

## What Geniuz does and doesn't do

- Your notes are stored in files on your own computer, under `~/.geniuz` unless
  `GENIUZ_HOME` names another directory. There is no account and no telemetry.
- Geniuz opens no network port. The AI connection (MCP) runs over standard input and
  output.
- It contacts GitHub to check for updates, and Hugging Face once to fetch the search
  model. Neither receives any of your notes. See `wiki/how-it-works.md`.
- A note you recall through an AI tool goes wherever that tool sends it.

## Checking a download

- **Mac:** the app is signed with a Developer ID and notarized. `spctl -a -vv /Applications/Geniuz.app`
- **Windows:** the installer is Authenticode-signed by Managed Ventures LLC.
  `Get-AuthenticodeSignature Geniuz-Setup.exe`
- **Linux:** each release publishes SHA-256 checksums. Signed releases are planned;
  the signing key's fingerprint is
  `C6B1 92E1 8E4F B1F8 60AD 1D0D 3CB2 1DF9 B572 C45F`
  (Managed Ventures LLC, Release Signing).

## Dependencies

`cargo audit` runs on every change to the dependencies and weekly. Any advisory that is ever set aside is listed,
with the reason, in `.cargo/audit.toml`.
