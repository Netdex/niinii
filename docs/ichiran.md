# ichiran

niinii runs `data/ichiran-cli.exe` against a PostgreSQL server in `data/pgsql`.
`ichiran-cli.exe` is an SBCL image with ichiran, its caches and the connection
settings baked in, so updating any of ichiran, its database or SBCL means
rebuilding it. `data/` is gitignored; this is the only record of how it is put
together. The runtime subset of it is packed into `deploy/ichiran-YYMMDD.7z`
(see [Packing](#packing)).

## Components

| Component | Source | In `data/` |
|---|---|---|
| PostgreSQL | Windows x86-64 binaries zip from <https://www.enterprisedb.com/download-postgresql-binaries> | `pgsql/` |
| ichiran database | `ichiran-YYMMDD.pgdump` from <https://github.com/tshatrov/ichiran/releases> | `pgsql/data/` (the dump is kept in `pgsql/`) |
| SBCL | `sbcl-X.Y.Z-x86-64-windows-binary.msi` from <https://sourceforge.net/projects/sbcl/files/sbcl/> | `sbcl/` |
| Quicklisp | <https://www.quicklisp.org>, already set up in `sbcl/quicklisp` | `sbcl/quicklisp/` |
| ichiran source | git checkout of <https://github.com/tshatrov/ichiran> | `sbcl/quicklisp/local-projects/ichiran/` |
| Conjugation tables | `conj.csv`, `conjo.csv` from <https://gitlab.com/yamagoya/jmdictdb/-/tree/master/jmdictdb/data> | `jmdict/` |
| KANJIDIC2 | `kanjidic2.xml` | `kanjidic2.xml` |

Last update (2026-11-27): PostgreSQL 19.0, database `ichiran-260118`, SBCL
2.6.10, Quicklisp dist 2026-01-01, ichiran `002a695` (2026-10-17). Before
that: PostgreSQL 17.2, `ichiran-250113`, SBCL 2.5.0.

## Layout

```
data/
  ichiran-cli.exe       built by (ichiran/cli:build); run with data/ as its working directory
  jmdict/               *jmdict-data*, relative to the working directory
  kanjidic2.xml
  pgsql/
    bin/ lib/ share/    from the EDB zip
    data/               the cluster (initdb)
    env.bat initdb.bat pgsql.bat
    ichiran-YYMMDD.pgdump
  sbcl/
    sbcl.exe sbcl.core contrib/   from the SBCL msi
    .sbclrc             loads quicklisp/setup.lisp, relative to the working directory
    sbcl-shell.bat      REPL with SBCL_HOME set; run it from data/sbcl
    quicklisp/local-projects/ichiran/settings.lisp
  archive/              previous versions
```

`settings.lisp` (gitignored in the ichiran checkout; copy it from the old one):

```lisp
(in-package #:ichiran/conn)
(defparameter *connection* '("jmdict" "postgres" "" "localhost" :port 5439))
(defparameter *connections* '((:old "jmdict_old" "postgres" "password" "localhost")
                              (:test "jmdict_test" "postgres" "password" "localhost")))
(in-package #:ichiran/dict)
(defparameter *jmdict-path* #p"/home/you/dump/JMdict_e")
(defparameter *jmdict-data* #p"jmdict/")
(in-package #:ichiran/kanji)
(defparameter *kanjidic-path* #P"kanjidic2.xml")
```

The port must match the one niinii starts Postgres on; niinii reads it back
from `ichiran-cli` at startup.

## Steps

Commands are PowerShell from `data/`. Move the current `ichiran-cli.exe`,
`pgsql/` and `sbcl/` into `archive/<old dump date>/` first, then build the new
ones in place. Don't build in a staging folder and move it: Quicklisp's
compiled files record absolute paths and break when moved.

### 1. PostgreSQL and the database

```powershell
Expand-Archive postgresql-<ver>-windows-x64-binaries.zip -DestinationPath .   # creates pgsql/
Remove-Item -Recurse pgsql\StackBuilder, pgsql\doc
Copy-Item archive\<old>\pgsql\*.bat pgsql\
Copy-Item ichiran-<date>.pgdump pgsql\

Set-Location pgsql
$env:PGLOCALEDIR = "$PWD\share\locale"
.\bin\initdb.exe -U postgres -A trust -D data
Start-Process .\bin\pg_ctl.exe -ArgumentList '-D','data','-o','"-p 5439"','-l','pg.log','start' -WindowStyle Hidden
.\bin\createdb.exe -p 5439 -U postgres -E UTF8 -l Japanese_Japan.932 -T template0 jmdict
.\bin\pg_restore.exe -p 5439 -U postgres -d jmdict --no-owner --no-privileges -j 8 ichiran-<date>.pgdump
.\bin\vacuumdb.exe -p 5439 -U postgres -d jmdict --analyze-only
```

Then set `timezone` and `log_timezone` to `'UTC'` in `data/postgresql.conf`.
initdb writes the build machine's timezone there, and the cluster is shipped.

- `pg_restore` reports one error, `schema "public" already exists`, and exits
  with 1. That is expected; upstream's scripts ignore it too.
- The dump carries no planner statistics; `vacuumdb --analyze-only` collects
  them.
- `pg_ctl start` run directly keeps a non-interactive shell open until the
  server stops, hence `Start-Process`. Stop it with
  `.\bin\pg_ctl.exe --wait -D data stop`.
- `initdb.bat` does the same steps interactively, without the analyze.

Leave the server running for the next steps.

### 2. SBCL and Quicklisp

```powershell
# Extracts without installing
msiexec /a "$PWD\sbcl-<ver>-x86-64-windows-binary.msi" /qn TARGETDIR="$PWD\sbcl-msi"
New-Item -ItemType Directory sbcl
Copy-Item "sbcl-msi\PFiles\Steel Bank Common Lisp\*" sbcl\ -Recurse   # sbcl.exe, sbcl.core, contrib
Copy-Item archive\<old>\sbcl\quicklisp, archive\<old>\sbcl\.sbclrc, archive\<old>\sbcl\sbcl-shell.bat sbcl\ -Recurse
Remove-Item -Recurse sbcl-msi

Set-Location sbcl
$env:SBCL_HOME = "$PWD"
Get-ChildItem -Recurse quicklisp -Filter *.fasl | Remove-Item   # compiled by the old SBCL
.\sbcl.exe --userinit .sbclrc --non-interactive --eval '(ql:update-client :prompt nil)' --eval '(ql:update-dist "quicklisp" :prompt nil)'
```

- SBCL refuses `.fasl` files from another SBCL version, and Quicklisp keeps its
  own next to its sources, so they have to go.
- Libraries in `quicklisp/local-projects` take precedence over the Quicklisp
  dist. The 2026-01-01 dist's named-readtables fails on SBCL 2.6
  ("Bug in readtable iterators or concurrent access?", melisgl/named-readtables
  #34), so a git checkout of it sits in `local-projects/named-readtables`.
  Remove it once a dist carries the fix.

### 3. ichiran

```powershell
Set-Location quicklisp\local-projects\ichiran
git fetch origin; git checkout origin/master
Remove-Item *.fasl -ErrorAction SilentlyContinue
Set-Location ..\..\..   # data/sbcl

# The image records the home, cache and temp paths it was built with; keep the
# user name and machine name out of it.
$h = "$PWD\build-home"; New-Item -ItemType Directory -Force "$h\tmp" | Out-Null
foreach ($v in 'HOME','USERPROFILE','APPDATA','LOCALAPPDATA','XDG_CACHE_HOME','XDG_CONFIG_HOME','XDG_DATA_HOME') { Set-Item "env:$v" $h }
$env:TEMP = "$h\tmp"; $env:TMP = "$h\tmp"
Remove-Item Env:HOMEDRIVE, Env:HOMEPATH, Env:USERNAME, Env:COMPUTERNAME, Env:USERDOMAIN -ErrorAction SilentlyContinue

.\sbcl.exe --userinit .sbclrc --non-interactive `
  --eval '(ql:quickload :ichiran)' `
  --eval '(ichiran/mnt:add-errata)' `
  --eval '(ql:quickload :ichiran/cli)' `
  --eval '(progn (cffi:close-foreign-library (quote cl+ssl/config::libssl)) (cffi:close-foreign-library (quote cl+ssl/config::libcrypto)))' `
  --eval '(ichiran/cli:build)'
Move-Item quicklisp\local-projects\ichiran\ichiran-cli.exe ..\
```

- `add-errata` brings the dump up to date with the checked-out code; run it
  every time the code or the database changes.
- `ichiran/cli:build` fills the caches from the database, so the server must be
  running and `settings.lisp` must point at it.
- cl-postgres depends on cl+ssl, which loads OpenSSL. The saved image reopens
  every DLL it had loaded at startup and dies with `Error opening shared object
  "libcrypto-3-x64.dll"` where OpenSSL isn't on `PATH`; niinii only reports
  `ichiran-cli worker 0 exited before READY`. ichiran connects without SSL, so
  the libraries are closed before the image is saved.
- Check the result with
  `Select-String -Path ..\ichiran-cli.exe -Pattern $env:USERNAME, 'Users' -SimpleMatch`
  run from a normal shell. The hits left after a clean build are upstream
  SBCL's build paths (`C:/sbcl-64/...`) and Lisp docstrings; ichiran's own
  symbols such as `DESU` look like a machine name but aren't.

### 4. Check

Run these with a bare `PATH`. A development shell (MSYS2, vcpkg, Git) supplies
DLLs that other machines don't have:

```powershell
$env:PATH = 'C:\Windows\System32;C:\Windows;C:\Windows\System32\Wbem'
.\ichiran-cli.exe -e '(princ (lisp-implementation-version))'   # from data/
Set-Location sbcl
.\sbcl.exe --userinit .sbclrc --non-interactive --eval '(ql:quickload :ichiran :silent t)' `
  --eval '(setf ichiran/dict::*jmdict-data* #p"../jmdict/")' `
  --eval '(ichiran/test:run-all-tests)'
```

From the repository root:

```powershell
cargo test -p ichiran -- --include-ignored   # the ignored tests need the database
cargo run --release --example inject_names -p ichiran
cargo run --release --example bench_parse -p ichiran -- 12
cargo test -p niinii
```

`run-all-tests` needs `*jmdict-data*` set because SBCL runs from `data/sbcl`,
not `data/`. The ichiran crate's tests deserialize ichiran's JSON, so they
catch output format changes. Stop the server afterwards; niinii starts its
own.

Also compare `jmdict/conj.csv` and `jmdict/conjo.csv` with jmdictdb's.

## Packing

`deploy/ichiran-YYMMDD.7z` (Git LFS) holds a ready-to-run `data/` with only what
niinii needs at runtime. It is extracted over the repository root.

| Path | Contents |
|---|---|
| `data/ichiran-cli.exe` | the clean build from step 3 |
| `data/jmdict/` | all of it (under 200 KB) |
| `data/ichiran-LICENSE.txt` | ichiran's `LICENSE` |
| `data/pgsql/bin/` | `postgres.exe`, `pg_ctl.exe` and the DLLs they import |
| `data/pgsql/lib/` | `*.dll` only (extensions and encoding conversions loaded on demand) |
| `data/pgsql/share/` | `timezone/`, `timezonesets/`, `tsearch_data/`, `extension/plpgsql*` |
| `data/pgsql/*.txt` | the EDB zip's `server_license.txt` and `commandlinetools_3rd_party_licenses.txt` |
| `data/pgsql/data/` | the cluster, without `postmaster.opts`, `postmaster.pid`, `log/`, `pg_stat_tmp/*` and `global/config_exec_params` |

niinii only runs `postgres` and `pg_ctl stop`. List the DLLs they import with
`objdump -p <exe> | grep "DLL Name"` (MSYS2 binutils), recursively, keeping the
ones that are in `bin/`. For PostgreSQL 19.0 that is `icudt78`, `icuin78`,
`icuuc78`, `libcrypto-3-x64`, `libiconv-2`, `libintl-8`, `liblz4`, `libpq`,
`libssl-3-x64`, `libxml2`, `libzstd` and `zlib1`. Everything else in `bin/`,
`include/`, `doc/`, `share/locale/` (message translations), the `.sql` and
`.sample` files in `share/` (used by initdb) and `lib/*.lib` is for building or
administering a cluster.

Before packing, with the server running:

1. `VACUUM (FULL, ANALYZE)` in `jmdict`. `add-errata` and test runs leave dead
   rows; check that `select count(*) from entry where seq >= 1000000000` is 0
   (names injected by tests).
2. Trim `pg_wal/`, which keeps up to `max_wal_size` (1 GB) of recycled segments:
   ```sql
   ALTER SYSTEM SET min_wal_size = '32MB';
   ALTER SYSTEM SET max_wal_size = '32MB';
   SELECT pg_reload_conf();
   -- repeat until pg_wal has a handful of segments; each checkpoint drops a few
   SELECT pg_switch_wal(); CHECKPOINT;
   ALTER SYSTEM RESET ALL;
   SELECT pg_reload_conf();
   ```
3. Stop the server with `pg_ctl stop` so `pg_controldata` reports
   `shut down`; a cluster copied while running needs recovery on first start.

Copy the files above into a staging folder, then test it the way niinii runs
it: start `data\pgsql\bin\postgres.exe -p 5439 -D data\pgsql\data` and run
`bench_parse` and `inject_names` with the staging folder as the working
directory, then `pg_ctl --wait -D data\pgsql\data stop`. That run writes to the
staged cluster, so copy the cluster in again afterwards. After packing, extract
the archive into an empty folder, put `niinii.exe` next to `data/` and run it
there with the bare `PATH` from step 4: it must open its window, start Postgres
and connect the ichiran workers. Scan the staging folder
for the user name, the machine name and the local timezone
(`grep -rla <name> <stage>`); `share/timezonesets` lists every zone and is
expected to match.

```powershell
& 'C:\Program Files\7-Zip\7z.exe' a -t7z -mx=9 -m0=lzma2 -md=256m -mfb=273 -ms=on -mqs=on -mmt=on ichiran-YYMMDD.7z data
```

Extracting needs 256 MB of memory for the dictionary. Replace the old archive in
`deploy/` with `git rm`, and update the name in `.github/workflows/rust.yaml`.
