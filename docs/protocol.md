# The Wysebee agent link: what Wysebee Agent for Linux speaks

How **Wysebee Agent for Linux** and **Wysebee Cloud** talk. Wysebee's agents for Mac and
Windows speak the same link; this file describes it as the Linux agent uses it. The agent
connects out over HTTPS and listens on no port: the cloud hands it calls through a long
poll, and it posts each answer back.

## The rules it keeps

- **It pairs with Wysebee Cloud only.** The person types an agent name on the machine, the
  agent shows a code, the person types the code in Wysebee (Devices), and pairing **creates
  a new agent** (or reconnects an agent of this type that has no machine now, when the
  person chooses so in Wysebee). Only that agent can use this machine.
- **Programs are set up in Wysebee and allowed here.** "Run local program" tools (a program,
  its fixed arguments and the blanks the model fills) live in the agent's settings in
  Wysebee Cloud, and run only once the person has allowed each one on this machine
  (`wysebee-agent allow`). The agent runs its own approved copy; **a call carries only the
  program's name and the values of its blanks.** Approval is never taken from the cloud.
- **Nothing is widened.** A program runs as an argument list (never a shell), with no
  input, in the person's home folder, stopped after 60 s, each output cut at 20,000
  characters.
- **Wider access is behind switches** the person turns on here (`wysebee-agent enable`):
  Run any program, Developer access and the Android toolkit. Each is off by default, asked
  to confirm, forgotten on unpairing and reported in the poll.
- **It runs no model.** The agent's model loop runs in Wysebee, which sends this machine's
  tools as the ops below.

## The cloud's address

The one saved with `wysebee-agent server <url>` (kept across restarts), else
`WYSEBEE_CLOUD_URL` when set, else `https://app.wysebee.com`; `wysebee-agent server default`
clears the saved one. It is set while unpaired: **a pairing keeps the cloud it was made
with**. An address must be `https`, or plain `http` to this machine (loopback, for
development): the secret and every call's values and output go there. All routes are under **`/api/v1/agent-links`**. The
agent's own routes carry its secret as `Authorization: Bearer <secret>`; redirects are not
followed, so the secret goes nowhere else.

## Pairing

1. The agent makes a **secret**: 32 random bytes, base64url without padding, kept in a file
   of its own (`~/.local/state/wysebee-agent/secret`, mode 0600, in the 0700 state folder).
   The cloud only ever learns its SHA-256.
2. `POST /api/v1/agent-links/start` (no auth)
   `{"kind": "wysebee-linux", "name": "<agent name: one line, 1-60>", "secret_hash": "<64 hex>",
   "browser": "<this machine's name, at most 32>", "extension_version": "x.y.z"}`
   -> `{"id", "code", "expires_in"}`. `kind` is the agent type; a cloud that does not know it
   answers 422 `invalid_kind`. 429 `too_many_starts` when this address started too many
   pairings in the last minute, or already holds too many codes nobody has typed yet.
3. The agent shows the code (`XXXX-XXXX`); the person types it in Wysebee, which claims it.
   The agent's poll (below) then answers `paired`.

## The poll

`POST /api/v1/agent-links/{id}/poll?wait=20` (Bearer), body `{"status": {…}}`:

| Field | |
|---|---|
| `paused` | bool: the person paused the agent; every op is refused. |
| `extension_version` | This agent's version. |
| `browser` | This machine's name. |
| `approved` | The names of the programs allowed here (at most 20). |
| `approved_skills` | The names of the skills allowed here (at most 50). |
| `folders` | The folders the person added here (below). |
| `any_program`, `dev_access`, `android_dev` | The switches. |
| `screen_control` | Always false on Linux. |

The answer is JSON with a `state`:

- `pending` `{"expires_in"}`: nobody has typed the code yet.
- `paired` `{"agent": {"name", "label"}, "programs", "skills", "calls", "cancelled"}`:
  - `programs`: the agent's "Run local program" tools, at most 20
    `{"name", "description", "command", "args": [{"value": "…"} | {"param": "…", "description": "…"}]}`;
  - `skills`: the agent's skills that have files, at most 50
    `{"name", "files": [{"path", "id": "<sha-256 hex>", "size"}]}`;
  - `calls`: `[{"id", "op", "args"}]`, the work to do; `cancelled`: ids of calls to stop.
- `refused` `{"message"}`: the claim was refused; start again.
- HTTP 401: unpaired in Wysebee (or the agent was deleted): the agent forgets the link,
  everything allowed here, the folders and the switches. HTTP 410: the code ran out.

The cloud wakes a waiting poll when the agent's settings are saved or a call arrives.

## Answering a call

`POST /api/v1/agent-links/{id}/calls/{call id}/result` (Bearer), body `{"ok": true,
"result": {…}}` or `{"ok": false, "error": "<an English sentence>"}`. A 404 (the call was
cancelled or expired) is dropped. Unpairing from the machine is `DELETE
/api/v1/agent-links/{id}` (Bearer); the agent stays in Wysebee, without this machine.

## Programs

**Approval.** A program is **the same** while its `name`, `command` and `args` (each fixed
value and each blank's name, in order) are unchanged; its description is not part of it. A
program with no approved copy, or whose copy differs, is **waiting**: `wysebee-agent
status` shows its whole command line, and a call to it is refused until the person allows
it. Allowing resolves the program **then** — an absolute path that is a file, or a name
found on the `PATH` plus the system's folders and the person's own (`~/.local/bin`,
Linuxbrew's, the Android SDK's `platform-tools`) — and that resolved path is what runs. A
program no longer in `programs` loses its approval. Approvals are kept in
`~/.local/state/wysebee-agent/`, never taken from the cloud.

**Ops:**

- `ping` -> `{"pong": true}`.
- `local_run` `{"name": "<program>", "values": {"<blank>": "<text>", …}}`
  -> `{"exit_code": <int|null>, "stdout", "stderr"}`, each cut at 20,000 characters with
  `\n…[cut at 20000 characters]`. Refused (`ok: false`) when paused, no such program,
  waiting for approval, a blank not filled, the program gone, not started, or not finished
  within 60 seconds (it is killed).

## Skills' scripts

A skill's `SKILL.md` may declare scripts in its front matter (`scripts:`, each a `name`, a
`command` and a `description`). **Only those scripts run, and only once the skill is
allowed here.**

- `GET /api/v1/agent-links/{id}/skill-files/{file id}` (Bearer) -> the file's bytes. The
  agent checks their SHA-256 is the id, and refuses the file otherwise. Paths must be
  relative, `/`-separated, with no `..`, no absolute path and no empty part.
- A skill is **the same** while its file list (each `path` and `id`) is unchanged, so its
  declared scripts are too. A changed skill waits; its files may be downloaded into a
  staging folder to show them, and **nothing in staging ever runs**. Allowing checks every
  hash, resolves each script's program (a path with `/` is a file inside the skill's
  folder, after following links; otherwise a program found as above) and puts the skill in
  place. Files the person added to the agent's own copy of the skill stay on the machine.
- `skill_run` `{"skill", "script", "args": ["<text>", …] (at most 30)}` -> as `local_run`.
  It runs the declared command's words then `args`, as an argument list, in the skill's
  folder, with no input, for at most 60 seconds.

## Folders

The person adds each folder here (`wysebee-agent folder add`), read only unless `--write`,
with an optional name and description. The cloud never holds a path the agent would act on.

- The poll reports them as `folders`: at most 20 `{"id", "name", "path" (shown only),
  "mode": "read" | "write", "description"}`. Wysebee shows the model each name, path and
  description as quoted data, never as instructions.
- `folder_call` `{"tool": "folder_find" | "folder_list" | "folder_read" | "folder_write",
  "folder": "<id>" | null, "args": {…}}` -> `{"text", "picture"?: {"media_type", "data"}}`.
  **The agent decides**: the folder must be its own; a path is relative to it, never
  absolute or `..`, and must still be inside it once links are followed; writing needs a
  read-and-write folder. Limits: files read up to 50 MB (a picture 10 MB), a search over at
  most 50,000 entries answering 200, a listing 500, sheets 5,000 rows, a write 1,000,000
  characters; `.git`, `node_modules`, `__pycache__` and the like are skipped. Text, Word,
  Excel/OpenDocument sheets and PDF are read as text. A picture comes back as `picture`, to
  be shown to the person and never given to the model.

## The switches

Each op below is answered only while its switch is on; otherwise it is refused in a sentence.

- **Run any program** (`any_program`): `program_run` `{"program": <1-1000 characters>,
  "args": [<text>, …] (at most 30)}` -> as `local_run`. **The AI names the program**, run as
  an allowed program is (any executable on the `PATH`, or a full path). A safeguard, not a
  sandbox, refuses disk, file-system and boot tools, shells and every program that runs
  another (`sudo`, `env`, `xargs`, `systemd-run`, `nsenter`, `busybox`, `find`, …), tools
  that change services, accounts, permissions, the kernel, the firewall or software
  (`systemctl`, `passwd`, `chmod`, `apt`, `snap`, …), power, `rm` and `kill`, interpreters
  given code inline, and git/rsync/journalctl arguments that destroy data.
- **Developer access** (`dev_access`), as the person, anywhere this account can reach:
  - `dev_shell` `{command, shell: "bash" | "zsh" | "sh" (default bash, bash and zsh as login
    shells), cwd, timeout_secs (1-220, default 120), background: bool}` -> `{exit_code,
    stdout, stderr}` (each cut at 30,000) or, in the background, `{job, status}`;
  - `dev_job` `{job, action: "output" | "stop" | "list", tail}`: at most 8 running, stopped
    on pause, when the switch goes off and on unpairing;
  - `dev_file` `{tool: "read" | "write" | "edit" | "glob" | "grep" | "list", path, …}` ->
    `{text}`.
- **The Android toolkit** (`android_dev`): `android_dev` `{tool, args}` through adb (on the
  `PATH`, `ANDROID_HOME`'s or `~/Android/Sdk/platform-tools`): devices, screenshots, the
  screen's layout, logs, apps, files, taps, typing, installing and a shell — never `su`,
  nor a reboot into the bootloader or recovery.

There is no screen control on Linux.

## What the cloud sees

Every call's blank values and output pass through Wysebee Cloud and are kept until read,
an hour at most. The cloud also holds the program definitions (they are the agent's
settings). It cannot make this agent run anything the person did not allow here.
