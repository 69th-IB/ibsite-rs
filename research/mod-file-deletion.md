# Can ibsite-rs delete a mod's files on the Arma server host so that the next Start downloads them again?

Research for [69th-IB/ibsite-rs#14](https://github.com/69th-IB/ibsite-rs/issues/14), read on 2026-10-16.

## Answer

Yes, without changes to the Arma3Server image.
Deleting `steamapps/workshop/content/107410/<id>` is enough.
The `appworkshop_107410.acf` manifest can stay as it is.
In a local SteamCMD test, `workshop_download_item` checked the item's files against its manifest on every run and downloaded the missing files again.
That test ran the Windows build of SteamCMD on a Space Engineers item, so the Linux build with an Arma 3 item is still unverified.

The web process deletes the directory through a short-lived container that it creates through `bollard`.
The container runs `rm -rf` on a bind mount of the host's workshop directory.
This needs no new mount in the web container and no change to the user the web container runs as.

The delete leaves the mod's keys in `/arma3/keys`.
The next Start copies the keys again, so stale keys do not block the download.
A key file cannot be traced back to one mod, because the image copies all keys into one flat directory and a later copy overwrites an earlier one with the same filename.

A delete runs only while Docker reports the Arma container as absent or not running.
It holds the same lock as Start and Stop, so no Start can create the Arma container while the delete runs.

## Sources and versions read

- [69th-IB/Arma3Server](https://github.com/69th-IB/Arma3Server) at `4c53976` (2022-08-01), the head of `main`. The files read were `Dockerfile`, `launch.py`, `workshop.py`, `keys.py`, `local.py` and `docker-compose.yml`.
- The Rails app at [69th-IB/ibsite@48f9e75](https://github.com/69th-IB/ibsite/tree/48f9e75), file `lib/arma/server.rb`.
- Valve's [SteamCMD page](https://developer.valvesoftware.com/wiki/SteamCMD) on the Valve Developer Community wiki, last edited 2025-11-08, read through the Internet Archive because the live page returns a bot check.
- SteamCMD for Windows from `steamcdn-a.akamaihd.net/client/installer/steamcmd.zip`, self-updated to client version 1791918946 on 2026-10-16. Its `logs/workshop_log.txt` and `logs/content_log.txt` are the evidence for the test below.
- The Docker Engine API spec, `api/swagger.yaml` on `moby/moby` `master`, API version 1.57.
- [bollard 0.21.1](https://docs.rs/bollard/0.21.1/bollard/struct.Docker.html), the latest release on crates.io.
- Python's [`glob` documentation](https://docs.python.org/3/library/glob.html), checked with a local run.
- Community tools that edit the manifest. These are [FyWolf/arma3-manager](https://github.com/FyWolf/arma3-manager) (`README.md`, `src/Support/SteamAcf.php`), [joka-de/slass](https://github.com/joka-de/slass) (`fn_removeWSItemFromStorage.sh`) and [German-Tactical-Ops/arma3server-docker](https://github.com/German-Tactical-Ops/arma3server-docker) (`notes/workshop.md`).

## What the image does at each Start

`launch.py` runs `steamcmd +force_install_dir /arma3 +login … +app_update 233780 … validate +quit` unless `SKIP_INSTALL` is set.
`workshop.py` then fetches the `MODS_PRESET` URL and extracts every `filedetails/?id=<id>` from the HTML.
It runs one SteamCMD process with `+force_install_dir /arma3`, the login, one `+workshop_download_item 107410 <id>` per mod and `+quit`.
It ignores SteamCMD's exit status.
For each mod it calls `keys.copy("steamapps/workshop/content/107410/<id>")` and adds the directory to the `-mod=` list.
`launch.py` prints the full command line as `LAUNCHING ARMA SERVER WITH …` and then runs the server with `os.system`.
SteamCMD has exited before the Arma server process starts.

The Rails app creates the container with the bind `<VOLUME_PATH>:/arma3`.
On the 69th box `VOLUME_PATH` is `/srv/docker/arma3/data` ([#7](https://github.com/69th-IB/ibsite-rs/issues/7)).
The `Dockerfile` declares `VOLUME /steamcmd` and no `USER`.
SteamCMD's own state therefore sits in an anonymous volume that each new container replaces, and every process in the container runs as root.
The workshop manifest sits under the install directory, at `/arma3/steamapps/workshop/appworkshop_107410.acf`, so the manifest persists across Starts on the host.

## Does SteamCMD download an item again when its directory is gone?

### What the primary sources say

Valve's SteamCMD page does not document `workshop_download_item`.
The page documents `validate` only as an argument of `app_update`.
SteamCMD is closed source, so no source-level evidence exists.

### What the community says

The FyWolf/arma3-manager README and its `SteamAcf.php` state that SteamCMD trusts the manifest over the disk.
They state that an item listed in the manifest with a current manifest id downloads nothing, so a deleted directory stays missing.
The tool removes the item's blocks from both `WorkshopItemsInstalled` and `WorkshopItemDetails`.
Its README also calls deleting the whole manifest "the folk remedy".
joka-de/slass deletes the whole manifest after it removes a workshop mission.
German-Tactical-Ops/arma3server-docker keeps a note that deletes the `temp` and `downloads` directories, with the manifest deletion commented out.
None of the three tools gives a SteamCMD version or a log that shows the failure.

### Local test

The test used Space Engineers (app 244850) because its workshop accepts an anonymous login.
The item was 3237810473, with 4 files and 3467646 bytes.
Each run was `steamcmd +force_install_dir <dir> +login anonymous +workshop_download_item 244850 3237810473 +quit`.

| Run | State before the run | Result |
| --- | --- | --- |
| 1 | Empty install directory | Downloaded. SteamCMD wrote `appworkshop_244850.acf` with the item in `WorkshopItemsInstalled` and `WorkshopItemDetails` |
| 2 | All files present | `Validating active workshop item`, then `0 updated, 0 moved, 0 deleted files` |
| 3 | Item directory deleted, manifest unchanged | `Validation: missing file` for each of the 4 files, `quick scan … found 4/4 mismatching files`, then `update started : download 0/905648 … stage 0/3467646` and `4 updated`. The directory was back with 3467646 bytes |
| 4 | One file overwritten with the same size and modification time | `0 updated`. The file kept the wrong content |
| 5 | Same as run 4, with `validate` after the item id | `0 updated`. The file kept the wrong content |
| 6 | One file deleted | `found 1/4 mismatching files`, then `1 updated`. The file was back |
| 7 | Item directory deleted, manifest unchanged, fresh SteamCMD directory | Same as run 3. This run matches the 69th container, where `/steamcmd` is new at each Start and the manifest persists on the host |

So SteamCMD client version 1791918946 runs a quick scan of the item's files on every `workshop_download_item`.
The quick scan finds missing files.
The quick scan did not find a changed file of the same size and modification time, with or without `validate`.
A delete of the whole directory is therefore the reliable way to force a fresh copy.
The manifest edit that the community tools perform was not needed in this test.
The community reports may come from older SteamCMD builds or from cases this test did not cover.

## How the web process can delete the files

The Docker Engine API has no call that deletes a file in a container.
`/containers/{id}/archive` supports `HEAD`, `GET` and `PUT` only.
`POST /containers/{id}/exec` runs a command "inside a running container", and the Arma container is stopped whenever a delete is allowed.

### Short-lived container through bollard (recommended)

The web process creates a container with these settings.

- `Image` is the Arma3Server image that ibsite-rs already uses. That image is already on the host and contains `rm`.
- `HostConfig.Binds` is `["/srv/docker/arma3/data/steamapps/workshop:/workshop"]`. The Docker API spec requires an absolute host path.
- `HostConfig.NetworkMode` is `none`.
- `Cmd` is `["rm", "-rf", "--", "/workshop/content/107410/<id>"]`, with `<id>` checked to be a decimal `u64` first.

The calls in bollard 0.21.1 are `create_container`, `start_container`, `wait_container` and `remove_container`.
`wait_container` returns the exit code.
The Docker API's wait `condition` accepts `not-running`, `next-exit` and `removed`, with `not-running` as the default.
The container runs as root because the image sets no `USER`, so it can delete the files SteamCMD wrote as root.
A fixed container name lets ibsite-rs find and remove a leftover job container after its own restart.

The host path of the data directory must be app config.
ibsite-rs needs the same path for the `/arma3` bind of the Arma container, so this adds no new setting.

### Bind mount into the web container

The web container would mount `/srv/docker/arma3/data/steamapps/workshop` and call `std::fs::remove_dir_all`.
The files are owned by root, so the web process would have to run as root in its container.
The mount reverses the decision in [#7](https://github.com/69th-IB/ibsite-rs/issues/7) that the web container needs no mount of the Arma data.
It also adds a deployment setting that has to match the host path in the app config.

### Delete at the next Start

ibsite-rs could also store a re-download mark and override the Arma container's `Cmd` at the next Start with a shell command that runs `rm -rf` and then `python3 /launch.py`.
This needs no second container and no running check.
It ties ibsite-rs to the image's `CMD`, and a failed delete appears only in the server log.

## Can the mod's keys be removed safely?

`keys.copy` runs `glob.glob(os.path.join(moddir, "**/*.bikey"))` without `recursive=True`.
In that form `**` matches a single directory level, so only keys one directory below the mod root are copied.
`shutil.copy2` copies each key into `/arma3/keys`, and a key with an existing filename overwrites the earlier file.
`keys.py` never removes a key.
`local.mods` calls the same `keys.copy` for every directory in `/arma3/servermods` and, with `MODS_LOCAL=true`, `/arma3/mods`.
`/arma3/keys` also holds keys that the 69th admins placed by hand ([#7](https://github.com/69th-IB/ibsite-rs/issues/7)).

A key file in `/arma3/keys` therefore carries no record of the mod it came from.
A key may be safe to remove only when no other directory under `content/107410`, `mods` or `servermods` ships a `.bikey` with the same filename.
Even then, ibsite-rs cannot tell a mod's copied key from a hand-placed key with the same filename.

Removing keys is not needed for the re-download.
The next Start copies the mod's keys again after SteamCMD restores the files.
A stale key matters only when a mod leaves every modlist for good, because the server then still accepts that key.
That cleanup is a separate feature and is outside this ticket.

## What a delete must check

1. Docker reports no Arma container, or an Arma container whose `State.Running` is false and whose status is `created`, `exited` or `dead`. The delete is refused for `running`, `restarting`, `paused` and `removing`, and when Docker is unreachable.
2. The delete holds the lock that Start and Stop take, from the state check until the job container is removed. Without the lock, a Start could create the Arma container while `rm` runs, and SteamCMD in the new container would write into the same directory.
3. The mod id is a decimal `u64`, so the path cannot leave `content/107410`.

A running server rules out every delete, not only deletes of loaded mods.
The modlist preset URL serves the modlist as it is when the container fetches it, so the modlist in the database may differ from the loaded set.
The only records of the loaded set are the `LAUNCHING ARMA SERVER WITH … -mod=…` log line and the server process's command line.
During the SteamCMD phase of a Start, SteamCMD writes into the workshop directory, and that phase shows as running in Docker.
A per-mod rule would need ibsite-rs to record the mod ids at Start, for example in a container label.
The Docker API cannot change a container's labels after creation, because `ContainerUpdate` accepts only resources and the restart policy.

## Unverified

- The Linux build of SteamCMD with Arma 3 (app 107410) and a logged-in account was not tested. A test on the 69th box settles it. Stop the server, delete `/srv/docker/arma3/data/steamapps/workshop/content/107410/<id>` for a small mod in the active modlist, leave the manifest, and Start. The Start log shows `Success. Downloaded item <id>` with the mod's size, and the directory is back. For the scan lines, read `content_log.txt` in the logging directory that SteamCMD prints at startup, before the next Start removes the container.
- The 69th box may run an image that differs from the head of 69th-IB/Arma3Server. The Rails app uses the image `frozendroid/arma3server`. `docker image inspect` on the box and a diff of `/workshop.py` in that image settle it.
- The test did not cover a partial download left in `steamapps/workshop/downloads/107410/<id>`. A delete can remove that directory as well at no cost.
