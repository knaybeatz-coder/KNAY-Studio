# AI control (local MCP)

The AI chip icon on the main toolbar opens the connection panel. AI is off at every launch, including when an assistant was remembered. There is no listener, worker, AI model, cloud request or audio analysis while off. The normal frame loop only checks whether a runtime exists.

## Connect

1. Enable AI in Lemon Studio.
2. Click **Save setup on Desktop**. It writes `Lemon Studio AI Setup/Personal MCP.json` or `Development MCP.json` plus instructions. These describe an MCP stdio server: the current Lemon executable with `--ai-mcp` and this profile's connection-file path.
3. Import/configure that server in a compatible assistant. This does not install or configure a particular AI product.
4. Connect, then approve the named client inside Lemon Studio within 60 seconds. The assistant cannot click its own approval or close the AI permission panel.
5. Turning AI off revokes sessions immediately. Restart the assistant connection after enabling again. Sessions expire after 15 minutes without a request; clients may ping to keep them alive.

Remember stores the client name and private connection key in this profile. It suppresses the approval only after the user manually enables AI. The name is self-declared, not a vendor certificate; trust is possession of the local key plus the approved name. Do not grant access to untrusted programs. Forget revokes the key and requires fresh approval. Personal and development profiles remain separate. The generated setup JSON does not contain the key.

No chatbot, model or subscription is bundled. Lemon does not transmit project data to a cloud; the selected assistant may send it to its provider. Assistant edits can modify the project and use its existing file/export dialogs. Human approval grants that scope for the session; it is not a confirmation for every edit.

## Coverage

Sixteen tools expose transport/tempo, project inspection, channel routing and levels, mixer levels/pan/mute/strip/FX/routing/Quick Sidechain, plugin insertion and exposed parameters, note editing, playlist clips, automation points, undo/redo, view scrolling, and discoverable app menu/dialog commands. Notes and clip positions use project ticks. Mixer faders use the host's normalized range (0.8 = unity); pan uses 0.5 = center. Slots are 1-based user slots, with the dedicated mini-EQ excluded from that numbering. Use returned hosted keys for plugin parameters.

`lemon_controls` returns command IDs and labels from the current interface without screenshots or mouse automation. Refresh after each edit and after the next paint. Pointer-drag/audio-keyboard surfaces are excluded: use semantic tools for notes, clips, routing and values. Existing menus provide access to additional commands; this is not a promise that every native plugin UI or every DAW feature has a dedicated semantic tool. Native plugin internals are only accessible when exposed to the host. There is no automatic audio listening or mixing-quality analysis.

Edits use the existing command and undo paths where those commands support undo. Native dialogs and loading states block incompatible edits. AI permission controls are never exposed. Frozen/comped clip moves are refused; use their existing track commands. Read current state before editing, preserve user work, and use Make Unique before changing the source of a shared clip.

## Transport and boundaries

The optional server binds IPv4 loopback on a random port and serves authenticated POST `/mcp`. A random 256-bit bearer key is required before requests reach the application. It rejects browser Origin headers, unexpected Host, duplicate headers, transfer encoding, oversized requests and wrong sessions. It has a bounded queue, at most four request workers, 20 tool requests per second, request deadlines and socket timeouts. UI-thread dispatch avoids concurrent mutation of the project. Turning off drops the queue; timed-out requests do not execute later.

The stdio adapter starts before DAW/audio initialization. It accepts newline JSON-RPC on stdin and writes only JSON-RPC on stdout. It reads the current endpoint from the profile, permits only `http://127.0.0.1:<port>/mcp`, and follows no redirects. This implements the JSON response subset of MCP 2025-06-18 Streamable HTTP, with stdio for client compatibility; no SSE subscription or remote-network server is provided.

Connection and remembered-key files live under the profile's `AI control` folder. Unix permissions are 0700 for that folder and 0600 for credential files; Windows inherits the user's profile permissions. Other programs running as the same user can read those credentials. This does not defend against a compromised OS account.

## Verification

Focused synthetic/headless tests cover HTTP authentication/Origin/session rejection, approval/decline, permission-panel isolation, command dispatch/revocation, note undo/redo, clip movement, routing selection isolation, input validation, FLP save/load and toolbar/menu discovery. Native assistant interoperability and hands-on production sessions still require user verification. No microphone or live session is used by these tests.
