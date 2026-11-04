# Lemon Studio feature voting service

The native browser tab uses https://michaellanemusic.de/lemon-studio/features/api.php.
The voting API and private owner dashboard are deployed independently of DAW installers/feeds.

## User behavior

Everyone can read. Each saved anonymous identity can vote independently on every suggestion. Repeated clicks are idempotent. Switching direction replaces the existing vote. Green selected buttons and a Your vote label show the choice. Suggestions are 8–120 characters. Three starter ideas are seeded with no invented votes. Results are ordered by net score, then newest; the board accepts at most 500 suggestions.

Networking runs on a worker, with an eight-second request timeout, no redirects, bounded JSON responses and minute refreshes only while the tab is visible. Offline uses cached results or starter suggestions, disables submitting/voting, and displays an inline status without modal errors. Draft text stays during the session; it is never automatically submitted later. Searching filters displayed titles.

## Identity and abuse limits

A server-issued 256-bit random bearer token is saved in the active profile's feature-voter.key before a vote is sent (0600 on Unix). Invalid/unreadable identity files fail closed rather than silently creating new identities. Concurrent writes cannot replace an existing identity. Never bundle this file in installers or source. The server stores only its SHA-256 hash, not the token. No hardware fingerprint, email, name, audio, project contents or plugin list is sent.

SQLite PRIMARY KEY(user,feature) enforces one current vote; BEGIN IMMEDIATE transactions serialize mutations and checks. Server-side limits: 120 requests/minute/IP network; 12 identity challenges/hour; 3 registrations/day/IP network; 40 changed votes/minute/identity; at most 3 new identities voting on the same suggestion/day/IP network; 3 submissions/day/identity and 6/day/IP network. IPv6 shares limits across its /64 network. This can affect shared networks. Creating an identity also requires a short, signed, expiring, IP-bound proof-of-work challenge, consumed once. No private signing secret is shipped in the client.

These are anonymous anti-abuse controls, NOT proof of a unique human. A determined attacker with multiple networks/identities can still manipulate votes. Verified accounts or managed bot challenges would be a future upgrade. Do not claim foolproof protection.

Input is length/type checked, normalized for duplicate titles, and rejects links/control characters/markup. SQL uses bound parameters; the native client renders text, not HTML. The endpoint accepts bounded JSON POST requests, no cookie credentials, and no cross-origin access. Server errors do not expose database paths or internals. Network limits store keyed hashes, not raw addresses; expired rate records are pruned. Standard hosting access logs may still contain IP addresses under the hosting provider's policy.

## Hosting and maintenance

Requires PHP 8.1+ and PDO SQLite. The database and private HMAC secret are created outside DOCUMENT_ROOT in its parent directory's lemon-feature-votes-private/votes.sqlite. It is not part of the downloadable source or app. Back up this directory privately using the host's backup facility; preserve the database across API deployments. The owner dashboard is at features/admin.php. It requires a separate owner password stored as a PHP password_hash in the private database; no credentials are included in the app or source. Do not delete or replace the live database during deployment.

api.php contains reusable core functions. test.php must run against a temporary isolated database, never the production database. It exercises repeat clicks, separate suggestions, direction changes, auth and input rejection, duplicate submissions, quotas, challenge replay and IP voting limits. A deployment can use a temporary unpredictable test path protected by a random request header; remove its files in finally. Upload the new API to a temporary filename then rename, retaining a private previous-code backup. Verify the live read-only list through HTTPS. Do not run registration/vote test traffic against the public board.

Reference: PHP random_bytes https://www.php.net/random-bytes ; SQLite transactions https://www.sqlite.org/lang_transaction.html .


## Owner moderation dashboard

Deploy admin.php, admin-core.php, admin.html, admin.js and admin.css alongside api.php. Never deploy test.php/admin-test.php as unprotected public routes. admin.php serves the static login shell; all data and moderation requests require owner authentication. Static UI files carry no secrets. No external JS, fonts, trackers or frameworks are used.

Authentication: random owner password, password_hash/password_verify, regenerated session ID after login, Secure/HttpOnly/SameSite=Strict session cookie, two-hour absolute session expiry, session CSRF token on every POST including login/logout, exact-origin check, bounded JSON requests and eight login attempts per IP per 15 minutes. Deny framing, use strict self-only CSP and no-store responses. Credentials and DB backups must remain outside DOCUMENT_ROOT. Login failures never reveal whether an owner is configured.

Controls: search/sort; hide/restore; close/reopen votes; public Open/Planned/In progress/Completed/Declined labels; paged voter identity review; block/unblock author or voter. Blocks prevent that anonymous identity submitting/voting and exclude its votes across all suggestions; unblocking restores them. Blocks do not hide its suggestions automatically. No destructive-delete button. Audit history is stored in moderation_log; dashboard displays latest 60 actions. Updated native clients show status labels and expandable owner comments (readable from offline cache too). Existing DAW clients remain compatible, display lock/blocked rejections inline and refresh the board normally.

The additive migration preserves existing SQLite tables/data. New columns: features.status/locked and users.blocked. Backup the private database with SQLite VACUUM INTO before deployment, then install files using staging names/renames. First owner provision must happen through trusted hosting administration or a random-header-protected one-use provisioning route, removed in finally. Refuse to overwrite an existing owner password; password resets require trusted hosting access. Do not put provisioning credentials in URLs, public files, Git, source snapshots or console output.

On Michael's Windows PC the Desktop shortcut launches Desktop/Lemon Studio Moderation/Open Dashboard.ps1. It reads a DPAPI-protected SecureString from LOCALAPPDATA/Lemon Studio Moderation/owner-password.xml, copies the password to the clipboard, and opens the HTTPS dashboard. The user pastes it into the login form. No token appears in the shortcut or URL. The private provisioning recovery copy lives in the Mac owner-only release staging folder, never shared source. A different account/computer requires deliberate credential setup. Instructions tell the user to replace clipboard contents after login and sign out when finished.

Verification: original test.php still passes. admin-test.php covers old-schema migration, visibility, locks, statuses, blocked identities/vote exclusion and restoration, audit, voter review, CSRF, expiry and login throttling in an isolated temporary DB. Deployment additionally exercises real HTTP unauthorized access, login, CSRF, foreign Origin, moderation, and logout against a protected temporary route, removing test files/storage afterwards. Production verification is owner login plus read-only board/list, with no fake live votes or moderation. Native clients must update to show public status labels/comments; older versions still vote normally.

Public comments: owner-only add/edit/remove, up to10 comments of600 Unicode characters per suggestion. Comments are plain text, authored as Michael / Lemon Studio, with creation/edit timestamps. Hidden suggestions never expose comments on the public board. Public API has no comment-write action; existing owner authentication and CSRF protect mutations. Additive public_comments table migration preserves votes/identities. Native HTTP response limit16MiB covers the bounded maximum board.
