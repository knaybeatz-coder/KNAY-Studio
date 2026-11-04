# JUCE Android demo configuration cleanup — 3 November 2026

GitHub alert #1 identified one upstream Google API key in three vendored JUCE Android demo google-services.json files. All three values matched the official juce-framework/JUCE demo at commit7c4601473a6f1bafcf691fd5a120bc4e30ad1bf7, project pushnotificationsdemorunner. GitHub reports validity Unknown; no attempt was made to use the key or access the Google project.

The api_key lists in these example configurations are now empty. They are not needed to build or run Lemon Studio or Master Assistant. Developers who run the separate JUCE Android push-notification examples must supply their own Google configuration. No audio/application binaries changed.

Public source export sanitizes these vendor demo configurations, rejects Google API key patterns elsewhere, and checks publication archives for unsanitized google-services.json files. A focused regression check and read-only CI scan protect future exports and commits. Existing release tags, historic commits and previously distributed archives are unchanged; this change does not revoke or rotate the upstream project's key. It is an upstream demo credential, not a Lemon Studio deployment credential. The alert should be classified as used in tests (upstream demo), not falsely marked revoked.
