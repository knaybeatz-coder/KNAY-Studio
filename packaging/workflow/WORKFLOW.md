# Lemon Studio shared workflow

Say **Build and publish the latest Lemon Studio** to the assistant on either computer. This authorizes the release; a question about a release does not.

Current installed publishing route: Windows sends a checksum-verified publication package over the home share, then invokes the Mac through the pinned `lemon-mac` SSH connection. The Mac uploads using certificate-verified explicit FTPS and its existing owner-only hosting credential file. The password stays on the Mac. Both machines must be awake and the share mounted. This route has passed an actual upload/replacement/HTTPS-download check; the new complete Mac application build remains a separate step. **Connect Publishing on Windows now checks the Mac connection; it does not ask for a hosting password.**

The home server stores the shared Git repository `Lemon Studio.git`, plus release snapshots, prepared packages and publication receipts. Each computer builds from its own local checkout. Never build inside the shared bare repository and never copy a Source directory over another working checkout.

## MacBook: one-time setup

Mount `smb://192.168.178.132/Home-Shared` in Finder. Inside `Storage/External-HD-Large-Files/Lemon Studio`, run `bash "Set Up Mac.command"`. It clones the current source to `~/Documents/Lemon Studio Source` and puts commands on the Desktop. Existing folders/changes are preserved. Native Rust 1.99.1, Python 3.11 or newer, Git and Apple Command Line Tools are prerequisites; the assistant should install/check them when setting up that Mac. Do not use Rosetta to build an Apple-silicon package.

For a different computer without the configured Mac relay or existing private credential-file reference, **Connect Publishing** supports a one-time direct FTPS login. The username is filled in; reachability is checked first. The password is stored only after the correct product folder is verified. This optional direct route is not required on the currently configured Windows PC/Mac. Do not copy the Mac's hosting password to Windows or the shared folder.

## Everyday work

1. Before editing, fetch from `home` and merge `home/main` with `--ff-only`. If local edits or diverging commits exist, preserve and reconcile them; never force-push or reset them away.
2. Make the requested changes, perform the relevant focused check, commit the intended source files, and push `HEAD:main` to `home`. Keep Mac fixes in this shared history too. Never push this edition to the upstream GitHub origin.
3. Run **Build and Publish**. The Windows command builds Windows/Linux; the Mac command builds the current native Mac architecture. It stops on dirty/divergent source, missing credentials, or unavailable server. Windows reserves a newer build number automatically. Mac uses the shared source version and must not overwrite an already-published different binary with the same name.
4. Versioned downloads are uploaded and checked before the platform links/feed change. Each architecture writes only its own links. Packages and receipts are copied to the home server. If upload fails, retain the prepared package and resume with `release.py publish <publish-plan.json>` rather than rebuild/reuse a published version.

The command runs only when invoked; it is not a background service and cannot start ChatGPT on another computer. Building/publishing a Mac binary must actually run on that Mac. Linux runtime/audio testing remains waived for the initial release; do not describe it as tested.

Common public link: https://michaellanemusic.de/lemon-studio/ . Passwords, Windows signing keys, personal projects, recordings and caches never go into the shared source. Source-only ZIP downloads are generated for each release. Windows, Linux x64 and native Mac builds use signed platform-specific automatic updates. Mac signing key stays privately on the Mac; Windows/Linux signing key stays on Windows. Mac updates replace a complete verified app bundle; Linux updates preserve executable permissions and app data. Install Mac in a writable Applications folder and extract Linux in a writable folder. Older Mac/Linux releases need one manual installation of the updater-enabled release.

If publishing stops after losing its connection, a `.publish-<platform>.lock` folder may remain on hosting. Verify no publisher is still running before removing that exact empty stale lock. Never delete versioned release files to force a new build through.

## Private Master Assistant hold (1 November 2026)

Michael explicitly prohibits publishing Master Assistant until he says it is ready. Ordinary builds exclude it. Never enable `private-master-assistant` for a public build. Windows/Mac/Linux packagers reject binaries carrying the private marker; public source ZIPs omit the private processor/UI and host integration. Private server/desktop recovery snapshots retain all source. Preserve Michael's installed local assistant bundle. Local app rebuilds for him use `cargo build --locked --release -p os-app --features private-master-assistant`; do not replace his private local app with a public build merely to prepare a release. A generic "publish updates" request does NOT lift this specific hold.
