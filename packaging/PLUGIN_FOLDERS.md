# Editable plugin folders

The plugin menus include **Organize plugin folders > Effects / Instruments**. Each user profile owns `Plugin database/Effects` and `Plugin database/Instruments`. Move or copy `.plugin.json` references into subfolders, then reopen the menu in Categories view. The stored plugin identity preserves correct loading after moving or renaming its reference file. These files are not plugin binaries or presets.

Startup and completed scans add newly discovered identities. Native plugins begin in default categories; other plugins begin in Unsorted. Rescans preserve moved/copied/deleted references. Keep `.known-plugins.json` to remember previously added identities. Plugins without a reference remain reachable under Other installed plugins. Malformed reference files are skipped. Name-only imported references resolve by normalized name and show disabled when unavailable.

Database files belong to AppData/user configuration, never the installation/source/installer. Existing user organization survives executable updates. Personal FL organization was a one-time local import; there is no runtime FL directory dependency. Public downloads use only the native defaults and each recipient's scanned plugins. Personal and Development profiles remain separate.

Nested plugin menus cascade alongside their parents. Clicking a folder opens another panel while earlier levels remain visible; clicking a sibling changes only that branch. Selecting a plugin dismisses the cascade. Panels open to the left when needed, and deeper wide menus retain separate scrolling.
