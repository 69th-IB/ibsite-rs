# Production data of the Rails ibsite and how to read it for migration

Research for [issue 4](https://github.com/69th-IB/ibsite-rs/issues/4). The source is the Rails app [69th-IB/ibsite at 48f9e75](https://github.com/69th-IB/ibsite/tree/48f9e75) (Rails 7.0.4, globalid 1.0.0, sqlite3 gem 1.6.0). Nobody read the production data for this note. Everything below comes from the schema, the app code and the Rails source at the pinned versions. The last section lists SQL checks that settle the open points against the real database.

The target database and the rich-text format are out of scope. Later tickets decide them.

## Answer

A migration reads three things from the `data/` volume of the Rails container:

- `data/production.sqlite3`, an ordinary SQLite 3 file with 15 tables.
- `data/storage/`, the ActiveStorage Disk service root, with one file per blob at `data/storage/<key[0..1]>/<key[2..3]>/<key>`.
- `data/master.key` (or `RAILS_MASTER_KEY`), which decrypts `config/credentials.yml.enc`. The data itself does not need this key.

No data column is encrypted. The ActionText sgids and the ActiveStorage signed blob ids in rich-text HTML are signed, not encrypted. Their payload can be decoded without `secret_key_base`. Verifying the signature needs `secret_key_base`, and a migration of its own trusted dump does not need to verify.

Of the 15 tables, `discord_roles`, `discord_role_members` and the Discord profile columns of `users` can be rebuilt from the Discord API. `discord_role_permissions` cannot, because it stores the site's own permission grants. The ActiveStorage variant tables and their blobs can be regenerated from the originals. The rest (missions, squads, slots, modlists, mods, server configs, users, rich text, the original blobs) must be migrated.

## Sources

Rails app (69th-IB/ibsite at 48f9e75):

- [db/schema.rb](https://github.com/69th-IB/ibsite/blob/48f9e75/db/schema.rb), schema version 2022_07_26_130320. No migration after July 2022 exists in `db/migrate`.
- [config/database.yml](https://github.com/69th-IB/ibsite/blob/48f9e75/config/database.yml) sets the production database to `data/production.sqlite3`.
- [config/storage.yml](https://github.com/69th-IB/ibsite/blob/48f9e75/config/storage.yml) defines the `local` Disk service with root `data/storage`. [config/environments/production.rb](https://github.com/69th-IB/ibsite/blob/48f9e75/config/environments/production.rb) line 41 selects it.
- [entrypoint.sh](https://github.com/69th-IB/ibsite/blob/48f9e75/entrypoint.sh) reads `data/master.key` into `RAILS_MASTER_KEY` and runs `db:setup` when `data/production.sqlite3` is missing. It does not set `SECRET_KEY_BASE`.
- [config/application.rb](https://github.com/69th-IB/ibsite/blob/48f9e75/config/application.rb) calls `config.load_defaults 7.0` and starts `Discord::UpdateRolesJob` at boot.
- Models in [app/models](https://github.com/69th-IB/ibsite/tree/48f9e75/app/models), the role sync in [app/jobs/discord/update_roles_job.rb](https://github.com/69th-IB/ibsite/blob/48f9e75/app/jobs/discord/update_roles_job.rb) and [lib/discord/discord.rb](https://github.com/69th-IB/ibsite/blob/48f9e75/lib/discord/discord.rb), the server start in [lib/arma/server.rb](https://github.com/69th-IB/ibsite/blob/48f9e75/lib/arma/server.rb).

Rails 7.0.4 source (rails/rails at tag v7.0.4) and globalid 1.0.0 (rails/globalid at tag v1.0.0). Each claim below links the file that owns it.

## Common encodings in the SQLite file

These apply to every table.

- Datetimes are TEXT in UTC as `YYYY-MM-DD HH:MM:SS` with an optional `.ffffff` microsecond suffix and no zone marker. Source: [abstract/quoting.rb `quoted_date`](https://github.com/rails/rails/blob/v7.0.4/activerecord/lib/active_record/connection_adapters/abstract/quoting.rb#L122-L136). The app does not change `default_timezone`, so it is `:utc`.
- Booleans are INTEGER `1` and `0`. Source: [sqlite3/quoting.rb](https://github.com/rails/rails/blob/v7.0.4/activerecord/lib/active_record/connection_adapters/sqlite3/quoting.rb#L32-L46).
- `t.string` columns have no length limit in SQLite. `server_configs.body` holds a whole Arma `server.cfg` in a string column.
- Primary keys are SQLite `INTEGER PRIMARY KEY` autoincrement ids.
- Polymorphic `record_type` columns hold Ruby class names. The values this app can produce are `Mission`, `Modlist`, `ModlistMod`, `Mod`, `ActionText::RichText`, `ActiveStorage::VariantRecord` and `ActiveStorage::Blob`.

Keeping the old primary keys avoids remapping foreign keys. Mission and modlist URLs (`/missions/:id`, `/modlists/:id`) appear in Discord posts. The Arma server container receives `MODS_PRESET=<webhost>/modlists/<id>/preset` ([lib/arma/server.rb line 67](https://github.com/69th-IB/ibsite/blob/48f9e75/lib/arma/server.rb#L67)). Kept ids keep those links valid.

## Per table

### users

Read every column except `remember_created_at`.

| Column | Written by | Migration |
|---|---|---|
| `discord_uid` | OAuth login, once | Migrate. This is the Discord user snowflake as a string and the identity key. |
| `discord_name`, `discord_discriminator`, `discord_avatar_url`, `discord_accent_color` | OAuth login, only on first login (`first_or_create` in [user.rb](https://github.com/69th-IB/ibsite/blob/48f9e75/app/models/user.rb#L39-L48)) | Rebuildable from Discord. The stored values are as old as each user's first login. Users who moved to Discord's new username system have discriminator `0`. |
| `discord_nick` | First login and every `UpdateRolesJob` run | Rebuildable from Discord. `NULL` means no server nickname. |
| `discord_color` | Nothing in the app | Expect all `NULL`. Drop. |
| `steam_uid`, `steam_nickname`, `steam_avatar_url` | Nothing. The Steam OAuth callback only prints the auth hash ([omniauth_callbacks_controller.rb](https://github.com/69th-IB/ibsite/blob/48f9e75/app/controllers/user/omniauth_callbacks_controller.rb#L18-L22)) | Expect all `NULL`. Check before dropping. |
| `time_zone` | Copied from a `time_zone` cookie that the browser sets with jstz ([application_controller.rb](https://github.com/69th-IB/ibsite/blob/48f9e75/app/controllers/application_controller.rb#L8-L18)) | Migrate, or let the next visit set it again. Values are IANA names such as `Europe/Amsterdam`, default `UTC`. |
| `remember_created_at` | Devise rememberable | Skip. Remember cookies are signed with the old `secret_key_base` and stop working at cutover anyway. |

`users.id` is referenced by `missions.creator_id`, `modlists.creator_id`, `slots.user_id` and `discord_role_members.user_id`. The users table itself must be migrated, even though most of its profile columns can be refreshed from Discord.

### discord_roles

Written only by `UpdateRolesJob` ([update_roles_job.rb](https://github.com/69th-IB/ibsite/blob/48f9e75/app/jobs/discord/update_roles_job.rb#L6-L29)). The job deletes roles that no longer exist in the guild and upserts the rest by `uid`. An upsert keeps the local `id` of an existing role.

- `uid` is the Discord role snowflake as a string.
- `name`, `position` and `admin` (the Discord Administrator permission bit) come from the guild role.
- `color` is a 6-digit lowercase hex string without `#`, or `NULL` when the Discord role color is 0.
- `hidden_for_permissions` is the only column the job does not write. No route or controller sets it, and the only UI for it is commented out in [admin/permissions/index.html.erb](https://github.com/69th-IB/ibsite/blob/48f9e75/app/views/admin/permissions/index.html.erb). Expect all `0`.

Rebuildable from Discord. The new app needs only the mapping from local `id` to `uid` at migration time, for `discord_role_permissions`.

### discord_role_members

A join table without its own meaning. `UpdateRolesJob` truncates it and refills it from the guild member list of each known user on every role or member event.

- `user_id` holds `users.id` and `role_id` holds `discord_roles.id`. Both are local ids, not Discord snowflakes.
- Both columns are declared `string`. Rails casts the integer ids to strings on insert, so they are stored as TEXT. A strict reader must parse them as text. SQLite's affinity rules still let `role_id = discord_roles.id` match in a join.
- Users who left the guild have no rows, and the job logs a warning for them.

Rebuildable from Discord. Do not migrate.

### discord_role_permissions

The site's own permission grants. Admins write them in the permission matrix ([admin/permissions_controller.rb](https://github.com/69th-IB/ibsite/blob/48f9e75/app/controllers/admin/permissions_controller.rb)). Discord does not hold this data, so it must be migrated.

- `role_id` is the local `discord_roles.id`, not the role snowflake. Join to `discord_roles` and migrate `(discord_roles.uid, key)` pairs, so the grants survive a rebuild of the roles table.
- `key` is one of the ten names in [role_permission.rb](https://github.com/69th-IB/ibsite/blob/48f9e75/app/models/discord/role_permission.rb#L4-L15): `manage_server`, `manage_modlists`, `create_missions`, `modify_others_missions`, `delete_missions`, `view_draft_missions`, `view_missions_early`, `enlist_self`, `enlist_others`, `override_enlistment`.
- When a role is deleted in Discord, `UpdateRolesJob` destroys its grants through `dependent: :destroy`. Grants for deleted roles therefore do not exist in the database.
- The Discord Administrator bit (`discord_roles.admin`) grants every permission in `User#can?`. It is not stored as grant rows.

### missions

Migrate all columns. `creator_id` references `users.id`. `start` is a UTC datetime and may be `NULL`. A model validation forbids `public = 1` together with `draft = 1`. The description is a rich-text row (`record_type = 'Mission'`, `name = 'description'`). The PBO is an attachment (`record_type = 'Mission'`, `name = 'pbo_file'`).

The PBO's original filename matters. The server start writes the file to `mpmissions/<filename>` and uses the filename without its extension as the Arma mission template name ([lib/arma/server.rb lines 25-38](https://github.com/69th-IB/ibsite/blob/48f9e75/lib/arma/server.rb#L25-L38)). A migration must keep `active_storage_blobs.filename` exactly.

### squads

Migrate. `parent_id` references another squad of the same mission and forms a tree. Root squads have `parent_id` `NULL`. `color` is empty, `NULL` or 6 hex digits without `#` ([squad.rb validation](https://github.com/69th-IB/ibsite/blob/48f9e75/app/models/squad.rb#L8-L9)). Squads have no ordering column, so display order is insertion order by `id`.

### slots

Migrate. `squad_id` references `squads`, and `user_id` (nullable) is the enlisted user. Slots have no ordering column either.

### modlists

Migrate. `creator_id` (nullable, no foreign key) references `users.id`. `parent_id` (nullable, no foreign key) references another modlist. `published_at` `NULL` means unpublished. The model declares a rich-text `description`, but no view renders or edits it, so expect no rich-text rows for `Modlist`.

### mods

Migrate, or rebuild from the Steam Workshop API by `workshop_id`. `title` and `file_size` come from the Steam `GetDetails` call when a mod is first added ([modlist.rb](https://github.com/69th-IB/ibsite/blob/48f9e75/app/models/modlist.rb#L28-L41)). The thumbnail is an attachment (`record_type = 'Mod'`, `name = 'thumbnail'`) fetched from the Steam preview URL.

- Current Workshop ids are above 2^31, and large mods exceed 2 GiB. Both columns need 64-bit integers in the target. SQLite stores both as 64-bit integers already.
- Every thumbnail blob has the filename `thumbnail.png`, whatever the actual image format. The `content_type` column holds the detected type ([mod.rb](https://github.com/69th-IB/ibsite/blob/48f9e75/app/models/mod.rb#L17)).

### modlist_mods

Migrate. Columns are `modlist_id`, `mod_id`, and nullable booleans `optional` and `server_only` with default `0`. The table has no unique index on `(modlist_id, mod_id)`. The "restore" action in [modlists_controller.rb](https://github.com/69th-IB/ibsite/blob/48f9e75/app/controllers/modlists_controller.rb#L77-L79) inserts without a duplicate check, so duplicates are possible. A rich-text `notes` is declared but never rendered or edited.

### server_configs

Migrate. Each row holds Arma server settings: `params`, `branch`, `creator_dlc` (a `;`-separated list such as `vn;ws`), `maxfps`, `headless_clients`, `modlist_id`, `mission_id`, `title`, and `body`, the raw `server.cfg` text. The server page and server start use only the newest row (`ServerConfig.last`).

### action_text_rich_texts

Migrate. Each row is `(record_type, record_id, name, body)`, unique per record and name. The only populated combination should be `('Mission', id, 'description')`. Section [ActionText body format](#actiontext-body-format) describes the `body` column.

### active_storage_blobs, active_storage_attachments, active_storage_variant_records

Migrate the original blobs and their attachments. Skip variants. Section [ActiveStorage layout](#activestorage-layout) has the details.

## ActiveStorage layout

### Files on disk

- A blob's file lives at `<root>/<key[0..1]>/<key[2..3]>/<key>` with no extension. Source: [disk_service.rb `path_for` and `folder_for`](https://github.com/rails/rails/blob/v7.0.4/activestorage/lib/active_storage/service/disk_service.rb#L99-L100).
- `<root>` is `data/storage` under the Rails root, which is `/app/data/storage` in the Docker image.
- `key` is 28 lowercase base-36 characters. Source: [blob.rb `MINIMUM_TOKEN_LENGTH` and `generate_unique_secure_token`](https://github.com/rails/rails/blob/v7.0.4/activestorage/app/models/active_storage/blob.rb#L39-L41).

### active_storage_blobs columns

- `filename` is the original upload name. `content_type` is detected from the content with Marcel.
- `byte_size` is the file size. `checksum` is the Base64 MD5 digest of the file ([blob.rb `compute_checksum_in_chunks`](https://github.com/rails/rails/blob/v7.0.4/activestorage/app/models/active_storage/blob.rb#L369-L376)). Both can verify each copied file.
- `metadata` is JSON text ([blob.rb line 42](https://github.com/rails/rails/blob/v7.0.4/activestorage/app/models/active_storage/blob.rb#L42)), for example `{"identified":true,"analyzed":true,"width":256,"height":256}` for an analyzed image.
- `service_name` is `local` for every blob.

### active_storage_attachments

Each row links a blob to a record by `(record_type, record_id, name)`. The combinations this app produces are:

| record_type | name | Meaning | Migrate |
|---|---|---|---|
| `Mission` | `pbo_file` | Mission PBO file | Yes |
| `Mod` | `thumbnail` | Workshop preview image | Yes, or refetch from Steam |
| `ActionText::RichText` | `embeds` | Files embedded in a rich-text body | Yes |
| `ActiveStorage::VariantRecord` | `image` | A resized image | No, regenerate |
| `ActiveStorage::Blob` | `preview_image` | A preview of a PDF or video embed | No, and probably absent |

`ActionText::RichText` has `has_many_attached :embeds`, and a `before_save` callback sets it to every blob referenced in the body ([rich_text.rb](https://github.com/rails/rails/blob/v7.0.4/actiontext/app/models/action_text/rich_text.rb#L15-L19)). Preview images need poppler or ffmpeg, and the [Dockerfile](https://github.com/69th-IB/ibsite/blob/48f9e75/Dockerfile) installs neither.

### Variants

`load_defaults 7.0` turns on `track_variants` (set in the 6.1 defaults) and selects the vips processor ([configuration.rb lines 189 and 246](https://github.com/rails/rails/blob/v7.0.4/railties/lib/rails/application/configuration.rb#L189)). Each variant is therefore an ordinary blob of its own, with a random key. It is attached as `image` to an `active_storage_variant_records` row ([variant_with_record.rb](https://github.com/rails/rails/blob/v7.0.4/activestorage/app/models/active_storage/variant_with_record.rb#L42-L48), [variant_record.rb](https://github.com/rails/rails/blob/v7.0.4/activestorage/app/models/active_storage/variant_record.rb)). The variant files sit in the same `ab/cd/<key>` tree as the originals.

`variation_digest` is the Base64 SHA1 of the Ruby-Marshal-dumped transformation hash ([variation.rb line 78](https://github.com/rails/rails/blob/v7.0.4/activestorage/app/models/active_storage/variation.rb#L77-L79)). It does not depend on any secret. The app produces these variants:

- `Mod` thumbnails, `resize_to_limit: [256, 256]` ([mod.rb](https://github.com/69th-IB/ibsite/blob/48f9e75/app/models/mod.rb#L6-L8)).
- Rich-text image embeds, `resize_to_limit: [1024, 768]`, or `[800, 600]` inside a gallery ([_blob.html.erb](https://github.com/69th-IB/ibsite/blob/48f9e75/app/views/active_storage/blobs/_blob.html.erb)).

A migration should select originals by excluding blobs whose only attachment has `record_type = 'ActiveStorage::VariantRecord'`, and generate new resized images in the new app.

### Orphan blobs

Replacing a `has_one_attached` file purges the old blob through a background job. Production uses the default in-process `:async` job adapter, so a restart can drop a pending purge. Trix also uploads a file before the form is saved, which leaves an unattached blob when the editor is abandoned. A migration should start from `active_storage_attachments` and not from the files in `data/storage`.

## ActionText body format

### What the column holds

`action_text_rich_texts.body` holds plain HTML, not YAML or JSON. `ActionText::Content.dump` stores `content.to_html` after canonicalizing it ([serialization.rb](https://github.com/rails/rails/blob/v7.0.4/actiontext/lib/action_text/serialization.rb), [content.rb](https://github.com/rails/rails/blob/v7.0.4/actiontext/lib/action_text/content.rb#L12-L25)). The HTML is Trix output: `<div>` blocks, `<br>`, `<strong>`, `<em>`, `<del>`, `<a href>`, `<h1>`, `<blockquote>`, `<pre>`, `<ul>` and `<ol>` lists. The `<div class="trix-content">` wrapper is added at render time by [the layout](https://github.com/69th-IB/ibsite/blob/48f9e75/app/views/layouts/action_text/contents/_content.html.erb), not stored.

Canonicalization turns each Trix `<figure data-trix-attachment>` into an empty `<action-text-attachment>` element ([trix_conversion.rb](https://github.com/rails/rails/blob/v7.0.4/actiontext/lib/action_text/attachments/trix_conversion.rb), [minification.rb](https://github.com/rails/rails/blob/v7.0.4/actiontext/lib/action_text/attachments/minification.rb)). Its possible attributes are `sgid content-type url href filename filesize width height previewable presentation caption` ([attachment.rb line 11](https://github.com/rails/rails/blob/v7.0.4/actiontext/lib/action_text/attachment.rb#L11)). A stored image embed looks like this:

```html
<action-text-attachment sgid="BAh7CEkiCGdpZAY6BkVUSSI..--0123abcd..." content-type="image/png"
  url="https://<host>/rails/active_storage/blobs/redirect/eyJfcmFpbHMiOn.../map.png"
  filename="map.png" filesize="123456" width="1920" height="1080"
  previewable="true" presentation="gallery"></action-text-attachment>
```

Two or more adjacent gallery attachments sit inside a bare `<div>` ([attachment_gallery.rb](https://github.com/rails/rails/blob/v7.0.4/actiontext/lib/action_text/attachment_gallery.rb)). An attachment with a `url` and an image content type but no resolvable sgid renders as a remote image ([remote_image.rb](https://github.com/rails/rails/blob/v7.0.4/actiontext/lib/action_text/attachables/remote_image.rb)).

Rails renders the stored HTML through a safe-list sanitizer ([content_helper.rb](https://github.com/rails/rails/blob/v7.0.4/actiontext/app/helpers/action_text/content_helper.rb#L17-L19)). The stored HTML itself is unsanitized user input. Whatever the new app does with it must sanitize it as well.

### How an sgid resolves

At render time Rails reads the `sgid` attribute and looks up the record ([attachable.rb `from_node`](https://github.com/rails/rails/blob/v7.0.4/actiontext/lib/action_text/attachable.rb#L10-L27)). When the lookup fails, the attachment renders as missing. The `url`, `filename` and `filesize` attributes are not used to find the blob.

The sgid is built as follows:

1. `attachable_sgid` calls `to_sgid(expires_in: nil, for: "attachable")` ([attachable.rb line 42](https://github.com/rails/rails/blob/v7.0.4/actiontext/lib/action_text/attachable.rb#L42-L44)). The sgid never expires.
2. `SignedGlobalID#to_s` signs the hash `{"gid" => "gid://ibsite/ActiveStorage::Blob/<blob id>", "purpose" => "attachable", "expires_at" => nil}` ([signed_global_id.rb](https://github.com/rails/globalid/blob/v1.0.0/lib/global_id/signed_global_id.rb#L58-L67)). The app part `ibsite` comes from the application name ([railtie.rb line 19](https://github.com/rails/globalid/blob/v1.0.0/lib/global_id/railtie.rb#L19)).
3. The verifier is a `GlobalID::Verifier`, an `ActiveSupport::MessageVerifier` with URL-safe Base64 ([verifier.rb](https://github.com/rails/globalid/blob/v1.0.0/lib/global_id/verifier.rb)). Its secret is `key_generator.generate_key("signed_global_ids")` ([railtie.rb line 29](https://github.com/rails/globalid/blob/v1.0.0/lib/global_id/railtie.rb#L29)).
4. `MessageVerifier#generate` serializes with Ruby `Marshal` by default. No purpose or expiry is passed to `generate` itself, so no JSON envelope is added ([message_verifier.rb](https://github.com/rails/rails/blob/v7.0.4/activesupport/lib/active_support/message_verifier.rb#L188-L204), [metadata.rb `wrap`](https://github.com/rails/rails/blob/v7.0.4/activesupport/lib/active_support/messages/metadata.rb#L18-L24)).
5. The result is `urlsafe_base64(Marshal.dump(hash)) + "--" + hex(HMAC-SHA1(secret, data))`.
6. The key generator is PBKDF2 over `secret_key_base` with 1000 iterations ([application.rb](https://github.com/rails/rails/blob/v7.0.4/railties/lib/rails/application.rb#L153-L158)). Under the 7.0 defaults it uses SHA256 ([configuration.rb line 228](https://github.com/rails/rails/blob/v7.0.4/railties/lib/rails/application/configuration.rb#L228)).

The part before `--` decodes without any secret. A Ruby Marshal dump stores strings as raw bytes, so the decoded bytes contain the literal `gid://ibsite/ActiveStorage::Blob/<id>`. This was read from the source and not tried on a real body.

The `url` attribute holds a blob redirect URL with an ActiveStorage signed id. That id comes from `ActiveStorage.verifier`, which is `app.message_verifier("ActiveStorage")` ([engine.rb](https://github.com/rails/rails/blob/v7.0.4/activestorage/lib/active_storage/engine.rb#L135-L137)). It is also signed, not encrypted, and wraps the blob id in a JSON envelope with purpose `blob_id`. The `url` also contains the host the editor ran on.

### Three ways to map attachments to blobs

1. Run an export inside the old container with `bin/rails runner`. The real `secret_key_base` is available there, and `rich_text.body.attachables` returns the blob records directly. This is the most reliable way and needs no Marshal parsing.
2. Decode the sgid payload outside Rails. Base64-decode the part before `--` and extract the `gid://ibsite/ActiveStorage::Blob/(\d+)` substring. This needs no secret and no Ruby.
3. As a cross-check, match the `filename` and `filesize` attributes against the blobs attached as `embeds` to the same rich-text row.

Whichever rich-text format the new app picks, the migration rewrites each `<action-text-attachment>` to point at the migrated blob. The sgid is only meaningful to a Rails app with the same `secret_key_base`.

## What can be rebuilt from Discord

| Data | Source in Discord | Rebuild or migrate |
|---|---|---|
| `discord_roles` (uid, name, color, position, admin) | Guild roles | Rebuild |
| `discord_role_members` | Guild member roles of each known user | Rebuild |
| `users.discord_nick` | Guild member nickname | Rebuild |
| `users.discord_name`, `discord_avatar_url`, `discord_accent_color`, `discord_discriminator` | Discord user object | Rebuild, and the result is fresher than the stored values |
| `discord_roles.hidden_for_permissions` | Not in Discord | Drop if all `0` |
| `discord_role_permissions` | Not in Discord | Migrate, keyed by role snowflake |
| `users` rows themselves | Discord knows the user, but not that the user ever logged in to the site | Migrate, because other tables reference `users.id` |

The rebuild needs the bot token and guild id from the credentials file. [Listing guild members](https://discord.com/developers/docs/resources/guild#list-guild-members) needs the privileged Server Members intent, while fetching one member by id does not. The Discord sync ticket covers the sync design.

## Secrets

| Secret | Where it lives | What needs it |
|---|---|---|
| Rails master key | `data/master.key` or `RAILS_MASTER_KEY` | Decrypting `config/credentials.yml.enc` |
| `credentials.discord.client_id`, `client_secret` | credentials | Discord OAuth login ([devise.rb](https://github.com/69th-IB/ibsite/blob/48f9e75/config/initializers/devise.rb#L274-L279)) |
| `credentials.discord.bot_token`, `ib_server_id` | credentials | The role and nickname sync |
| `credentials.steam.api_key`, `username`, `password` | credentials | Workshop lookups and the Arma server's SteamCMD login |
| `secret_key_base` | `SECRET_KEY_BASE` env var, else `credentials.secret_key_base` ([application.rb line 420](https://github.com/rails/rails/blob/v7.0.4/railties/lib/rails/application.rb#L415-L420)) | Verifying or creating sgids and signed blob ids, and session and remember cookies |

The entrypoint does not set `SECRET_KEY_BASE`, so `secret_key_base` most likely lives in the credentials file. The deployment's environment would settle that.

No table column needs the master key or `secret_key_base` to decode. The new app needs the credentials file decrypted once, to carry over the Discord and Steam secrets. Existing logins do not survive cutover in any case, because the session cookie is encrypted with the old `secret_key_base`.

## Risks

- ActionText attachments resolve only through the sgid. A migration that keeps the HTML unchanged and drops the Rails app loses every embedded file reference. Each `<action-text-attachment>` must be rewritten.
- Stored rich-text HTML is unsanitized user input. A renderer that outputs it without a safe-list sanitizer opens an XSS hole.
- `discord_role_permissions.role_id` is a local id. Rebuilding `discord_roles` from Discord without first mapping the grants to role snowflakes loses every permission grant.
- `mods.workshop_id` and `mods.file_size` overflow 32-bit integers.
- The mission PBO filename is part of the Arma server interface. Renaming the file during migration changes the mission template name.
- The blob table can contain orphans and variants. Copying every file under `data/storage` copies dead files as well.
- `discord_role_members` stores integer ids as TEXT, so a strictly typed reader fails on those columns unless it reads them as strings.

## Checks to run against production

These read-only queries settle the open points. They run with `sqlite3 data/production.sqlite3` on a copy of the file.

```sql
-- Which attachment kinds exist, and how many
SELECT record_type, name, count(*) FROM active_storage_attachments GROUP BY 1, 2;

-- Which rich-text kinds exist
SELECT record_type, name, count(*) FROM action_text_rich_texts GROUP BY 1, 2;

-- Rich-text attachments whose content is not a blob (remote images, other attachables)
SELECT id FROM action_text_rich_texts
WHERE body LIKE '%<action-text-attachment%' AND body NOT LIKE '%sgid=%';

-- Blobs without any attachment (orphans)
SELECT count(*), sum(byte_size) FROM active_storage_blobs b
WHERE NOT EXISTS (SELECT 1 FROM active_storage_attachments a WHERE a.blob_id = b.id);

-- Columns expected to be empty
SELECT count(*) FROM users WHERE steam_uid IS NOT NULL OR discord_color IS NOT NULL;
SELECT count(*) FROM discord_roles WHERE hidden_for_permissions = 1;

-- Permission grants, keyed by role snowflake
SELECT r.uid, r.name, p.key FROM discord_role_permissions p JOIN discord_roles r ON r.id = p.role_id;

-- Duplicate mods within a modlist
SELECT modlist_id, mod_id, count(*) FROM modlist_mods GROUP BY 1, 2 HAVING count(*) > 1;

-- Value ranges that need 64-bit columns
SELECT max(workshop_id), max(file_size) FROM mods;
```

A file check confirms the disk layout. For each blob key, `data/storage/<key[0..1]>/<key[2..3]>/<key>` exists, its size equals `byte_size`, and its Base64 MD5 equals `checksum`.
