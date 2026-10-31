# Michael Lane Windows automatic updates

The top-right Update button checks manually; normal application startup checks in a background thread and repeats every six hours. Downloads do not touch the audio thread. A verified update installs after a normal close, or via Update when its status says an update is ready after playback stops and the usual save-changes prompt is answered. Cancel keeps the session open. Restart reopens the saved project when available.

The updater supports per-user Windows x64 installations with write access. It has no service, scheduled task, elevation prompt or telemetry. HTTPS requests necessarily disclose the requesting IP to the website host. Offline/check failures leave the running app untouched. The download service is https://michaellanemusic.de/lemon-studio/updates/ . Existing older builds without this updater require the new installer once.

The manifest's exact UTF-8 payload is signed using a dedicated Ed25519 key. The app pins its public key, product and monotonically increasing build number, then checks each file's length and SHA-256. The helper rechecks before installing. This is update authentication, not a Windows Authenticode certificate; the initial installer remains unsigned. Private signing authority is never included in source or downloads.

Updates replace only lemonstudio.exe, Lemon Studio Source.zip, LICENSE and THIRD-PARTY-NOTICES.txt. The installed runtime DLL set is unchanged by this protocol. A future change requiring different runtime files must explicitly extend and test the protocol before distribution. Projects, settings, templates and unrelated files are excluded. Originals are retained in .update-backup-* beside the application. File replacements are atomic per file; a failed transaction attempts rollback. An abrupt power loss can leave mixed resource versions, although the executable replacement itself is atomic; backups remain for recovery. This is not crash-health monitoring or automatic rollback of a release with an audio/UI regression.

Downloads and helper result.txt live under %LOCALAPPDATA%/Lemon Studio/Updates. The helper waits for the specific parent process to exit and never terminates a session. Failed or locked installations leave backups and a diagnostic result. The update cache and old backups are retained rather than deleting user-visible files automatically.

## Preparing the next update

1. Finish the requested changes and focused checks. Increase VERSION in crates/os-app/src/updates.rs and the NSIS DisplayVersion. Keep the established public key.
2. Build with OPENSTUDIO_NO_PLUGIN_BUILD=1 and cargo build --locked --release -p os-app. Refresh corresponding source and licence notices when dependencies change.
3. Run packaging/windows/prepare-update.py --output ABSOLUTE_STAGING_FOLDER. It requires Python cryptography and the owner's private update key outside the repository. It verifies that the built app reports the source build number and checks the signed output.
4. Test the release before publication. Upload updates/BUILD_NUMBER/* first into the dedicated website directory. Verify actual HTTPS downloads against the signed manifest. Only then replace updates/stable.json to announce the release to installed clients. Do not overwrite an existing version's payload.
5. Build the full NSIS installer for new users and retain the corresponding source archive. The updater does not publish development builds automatically. Publishing a new signed stable manifest distributes that approved release to users.

Mac auto-updating is not implemented or advertised by this Windows release.
