# Trawler router command templates

`M=/Users/nguyenvanluong/Documents/proj/misc` · `WT=$M/Trawler-<slug>` · main checkout `$M/Trawler`.
Owner commands are printed for the owner, never run by the router. Always print
the full 40-char sha and an absolute `cd` prefix.

## Router: verify before relaying (read-only)

```sh
cd $WT && git branch --show-current && git rev-parse HEAD && git status --short | head -3
git merge-base --is-ancestor epic/<slug> HEAD && echo based-on-epic
for r in test doc cr design mr; do echo "$r: $(git notes --ref=$r show HEAD 2>&1 | head -1)"; done
ls docs/work/<result>.html
cargo xtask verify-dod wu HEAD --target epic/<slug>     # read-only dry run: only 8·manual-review red
actionlint .github/workflows/*.yml; echo actionlint=$?  # when workflows changed
git -C $M/Trawler log --oneline -3 main; git -C $M/Trawler rev-parse main origin/main; git tag | tail -3
```

## Owner: rule-bearing doc commit (T0/T1/DEC)

```sh
cd $WT && git diff --cached --name-only        # router checks: only the intended docs are staged
cd $WT && cargo xtask mr commit -m $'<E-CODE.Wn>: <subject>\n\n<body; escape apostrophes as \'>' --as vuluu
```

## Owner: WU sign and merge

```sh
cd $WT && open docs/work/<date>-<epic>-w<n>-<slug>-result.html
cd $WT && cargo xtask mr confirm wu <full tip sha> --as vuluu --target epic/<slug> --report docs/work/<date>-<epic>-w<n>-<slug>-result.html
cd $WT && git switch epic/<slug> && git merge --no-ff wu/<epic-slug>/<slug>
```

The epic branch must not be checked out in another worktree (remove temp worktrees first).

## Owner: epic sign, merge to main, push

```sh
cd $WT && open docs/.archive/<date>-<slug>-epic-result.html
cd $WT && cargo xtask mr confirm epic <full tip sha> --as vuluu --target main --report docs/.archive/<date>-<slug>-epic-result.html
cd $WT && cargo xtask verify-dod epic epic/<slug>                 # green after the signature: the merge hook runs the same gate
cd $M/Trawler && git merge --no-ff epic/<slug>
cd $M/Trawler && git push origin main
```

## Owner: release tag (tag the verified tip; main not required)

```sh
cd $WT && grep -m1 '^version' Cargo.toml                          # must equal the tag without "v"
cd $WT && git tag v<version> <full tip sha or epic/<slug>> && git push origin v<version>
```

Never move or delete a pushed tag; bump the version (`0.1.0-N`) for the next attempt.

## Owner: outward-facing CI steps

```sh
cd $WT && git push origin <branch>
cd $WT && gh workflow run ci.yml --ref <branch>
```

## Router: watch runs (one background command; filter by sha)

```sh
until id=$(gh run list --workflow ci.yml --branch <br> --limit 1 --json databaseId,status,headSha -q '.[0] | select(.status=="completed" and (.headSha|startswith("<sha8>"))) | .databaseId') && [ -n "$id" ]; do sleep 45; done; echo "RUN $id"; gh run view $id --json conclusion,jobs -q '.jobs[]|"\(.name): \(.conclusion)"'
until [ "$(gh run view <id> --json status -q .status)" = "completed" ]; do sleep 120; done; gh run view <id> --json conclusion,jobs -q '"run: \(.conclusion)", (.jobs[]|"\(.name): \(.conclusion)")'
gh run list --workflow release.yml -L 3 --json databaseId,headBranch,status,conclusion -q '.[]|"\(.databaseId) \(.headBranch) \(.status) \(.conclusion)"'
```

## Router: spawn / retire a worker

```sh
S=<router dir>; C=$(ps -axo command | grep -m1 -o '/[^ ]*native-binary/claude')   # or `command -v claude`
ls "$C" "$S/briefs/<brief>.md" && cd $WT && git branch --show-current && git status --short | wc -l && \
(nohup "$C" -p "You are a fresh worker session. Read $S/briefs/<brief>.md in full and do exactly what it says in one non-stop run. Router is <router-name>. First action: git branch --show-current, git log --oneline -6, git status --short. Report with SendMessage to <router-name>." \
  -n trawler-<area>-<phase> --model claude-opus-5-5 --permission-mode auto > "$S/trawler-<area>-<phase>.out" 2>&1 &); echo started
ps -axo pid,command | grep -E 'claude.* -p ' | grep -v grep | grep -o -E '\-n trawler-[a-z0-9-]+ .*' | cut -c1-90   # name + --model visible
P=$(pgrep -f -- '-n trawler-<area>-<phase> ' | head -1); [ -n "$P" ] && kill "$P"                                   # only idle + clean
```

The IDE-bundled binary's path carries the extension version, so it is found, never hard-coded.

## Router: a new parallel WU worktree (the worker creates it)

```sh
git -C $M/Trawler worktree add ../Trawler-<short> -b wu/<epic-slug>/<slug> epic/<slug>
```

## Router: retire a closed worktree (disk)

```sh
cd $M && b=$(git -C Trawler-<slug> branch --show-current); git -C Trawler merge-base --is-ancestor $b main && echo merged
git -C Trawler-<slug> status --short | wc -l                                  # 0
ps -axo pid,command | grep 'Trawler-<slug>' | grep -v grep                    # nothing
cp -Rn Trawler-<slug>/docs/.archive/. Trawler/docs/.archive/
for f in $(cd Trawler-<slug>/docs/.archive && find . -type f); do cmp -s "Trawler-<slug>/docs/.archive/$f" "Trawler/docs/.archive/$f" || echo "DIFF $f"; done
git -C Trawler worktree remove ../Trawler-<slug> && git -C Trawler worktree prune   # keep the branch (notes live on it)
```

## Owner try-recipe guards (real data)

```sh
mv ~/Library/Application\ Support/dev.trawler.app/keep-capturing.json ~/kc.bak      # before
mv ~/kc.bak ~/Library/Application\ Support/dev.trawler.app/keep-capturing.json      # after
lsof -nP -iTCP:7420 -iTCP:7421 -iTCP:4445 -sTCP:LISTEN                                  # quit other Trawlers first
```
