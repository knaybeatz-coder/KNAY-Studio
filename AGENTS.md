# Michael Lane edition: cross-computer source and releases

This is the customized source, not a disposable upstream checkout. Preserve the user's work. Questions alone are not authorization to edit/build/publish. For authorized release work read packaging/workflow/WORKFLOW.md.

- `home/main` on the private home server is the shared source of truth. Keep this checkout local; never build in the shared bare repository. Fetch and fast-forward before edits; stop and reconcile local edits/divergent commits instead of stashing/resetting/force-pushing.
- Commit relevant source changes and push `HEAD:main` to `home` after authorized updates, including Mac fixes. Never push the customized edition to the upstream GitHub origin.
- For "Build and publish the latest Lemon Studio", use packaging/workflow/release.py build-publish with the configured private Python environment. On Mac, packaging/workflow/Build and Publish.command sets up that environment. Windows builds Windows/Linux; Mac builds the actual Mac architecture. Preserve other platforms' download manifests.
- Keep passwords in the OS credential store; private configuration under ~/.lemon-studio-release. Never put credentials, private keys, personal projects/recordings or build caches into Git, public source or the shared source folder.
- No silent microphone tests, force-closing sessions or discarding projects. No GUI control unless authorized. Keep finished products on Desktop. Do not claim a platform was tested or published unless that actually succeeded.
- Update machine handoff notes after material changes. A successful Git push is source synchronization, not public publication.
