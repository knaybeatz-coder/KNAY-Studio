# Lemon Studio projects (.lmn)

New projects, Save As, Save New Version, new project-folder saves, new templates, autosave backups and projects exported inside ZIP files now use .lmn. The underlying lossless project serialization is unchanged. This is a native filename/association change, not a new binary codec.

Open accepts .lmn, legacy .flp and zipped projects. Drag-and-drop accepts both project extensions. Regular Save of an already-open .flp retains that existing filename. Save As proposes .lmn and Save New Version writes a new .lmn sibling. Existing files are never batch-renamed. Backup recovery recognizes both extensions; template discovery prefers .lmn and falls back to an existing .flp template. Unused-sample cleanup protects both project file types.

Windows settings register .lmn under LemonStudio.LmnProject, separately from previous .flp associations. The installer adds Open With and assigns .lmn only when no per-user default exists. It does not overwrite a Windows UserChoice. Uninstall removes its association only if its command still points to this installed executable. Mac bundles advertise both .lmn and .flp. Linux users can open both through Lemon Studio's file dialog; no desktop-wide MIME changes are made by the portable archive.

The voting browser now reads the server's locked field, labels closed suggestions Voting closed and disables both voting controls. Older cached boards without the field remain readable. Server moderation/password controls stay outside the DAW; no owner credential is packaged.
