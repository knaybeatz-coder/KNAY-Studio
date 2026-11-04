# Local public-preview profile

The dedicated `lemonstudio-development` executable uses **Lemon Studio Public Preview** under the platform's normal application-data folder. This applies to a direct executable launch as well as the Desktop development launcher. An explicit `OPENSTUDIO_USER_DIR` still overrides it. The preview does not migrate a legacy OpenStudio profile. Public executable names retain the normal Lemon Studio profile.

A fresh preview uses the actual embedded Lemon Starter template and normal application defaults. Samples default to this profile's own `Samples` directory; no machine-specific sample drive is guessed. Existing personal sample locations belong in the `browser_samples_root` preference. An empty library says **No samples yet**. Samples, favorites, settings, plugin database, downloaded stem runtime/headphone profiles and AI trust are separate from the personal installation.

On Michael's local Windows installation, the preview is seeded with an empty plugin cache and `startup_scan=false` so testing on a producer's computer simulates a new user with no additional plugins. This is the only deliberate preview preference override; public users still discover their own installed plugins. No sample packs are bundled in the current release payload. Users add their folders themselves.

The profile is persistent after its first launch: subsequent testing can change it. Do not reset it on each launch or delete test projects. Preserve the previous development profile when preparing a fresh first-run preview. Never ship local profiles, private plugin-import files, personal libraries or preview launcher configuration in public packages.

Fresh installations also start with the browser on the left, mixer across the top (about62% of the workspace), and playlist below. The channel rack and piano roll start hidden. Geometry is applied at the first main-window paint so it uses actual display dimensions, including a435px minimum mixer height for720p layouts. Both window heights and their shared divider remain adjustable. A saved mixer or playlist rectangle prevents the first-run arrangement from replacing the user's layout.
