# MacBook release workflow

The preferred workflow now uses the shared Git repository and `packaging/workflow/Build and Publish.command`. Read `packaging/workflow/WORKFLOW.md` first. On the home server run `Set Up Mac.command` to clone the shared repository locally. ZIP snapshots and the KeyHelp upload steps below remain fallback/recovery methods; do not replace a current Git checkout with an older snapshot.

The Windows PC builds Windows and Linux. Build Mac natively on the MacBook. Do not advertise a Mac binary before the native build completes. The common public URL is https://michaellanemusic.de/lemon-studio/ .

The current Windows source is on the home server, `smb://192.168.178.132/Home-Shared/Storage/External-HD-Large-Files/Lemon Studio`, under versioned `releases`. Read `LATEST.json` and verify its source SHA-256. Copy the source ZIP to local storage and extract it into a fresh working folder. Preserve any existing Mac changes and reconcile them before building; never overwrite a Mac checkout blindly. Do not build directly on the network share.

For an authorized Mac update, ChatGPT should finish the build and website publication as one task. Install native Rust 1.99.1 and Apple Command Line Tools if absent, then run `bash packaging/mac/build-release.command` from the extracted Source directory. The resulting DMG and publication ZIPs are under Desktop/Lemon Studio Mac Public Download. The architecture is detected, not assumed. No universal-binary claim. The app is ad-hoc signed, not Apple-notarized.

Use the existing authorized hosting login at https://web12.webprojaggt.de and File manager. Website root: `/home/users/michaellane-music/www/michaellanemusic.de`. Upload and extract the `files.zip` there. Download both new files over HTTPS and compare their SHA-256 values with local artifacts. Then upload/extract `publish-last.zip`. Verify the Mac choice appears at the common download URL and save a screenshot. Keep older version files. Never upload a Windows feed, index.html, Windows/Linux manifest, or private signing keys as part of a Mac-only publication.

The landing page reads `platforms/mac-arm64.json` and `platforms/mac-x86_64.json` independently, so publishing one architecture preserves the other and all Windows/Linux downloads. Do not copy a different platform's source link into a Mac manifest. The package script exports the exact local source used for that Mac build, including any reconciled Mac fixes.

Copy the released Mac DMG, matching source, manifest, hashes and a short status note back to the home server under `Mac releases/<version>/<architecture>`. Never replace `LATEST.json`, which identifies the current Windows source handoff. Update the Mac workspace handoff after publication.

These are persistent workflow instructions, not a running background agent. ChatGPT must be running on the Mac with hosting access to build and publish. This Windows PC cannot compile or silently start a MacBook ChatGPT session. Do not claim automatic deployment while a login or native Mac build is still missing.

Automatic updates: releases now include an architecture-specific Ed25519-signed feed prepared by packaging/prepare-unix-update.py. Use packaging/workflow/release.py build-publish so the signed payload and feed are included. The private mac-update-signing-key.pem stays on the Mac; only its verification key is in source. Mac app installation requires a writable Applications folder (~/Applications is supported), closes normally before bundle replacement, verifies code signing and retains a hidden rollback bundle. Older Mac versions need one manual upgrade.
