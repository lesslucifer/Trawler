# Trawler command templates

`M=/Users/nguyenvanluong/Documents/proj/misc` · `WT=$M/Trawler-<slug>` · main checkout `$M/Trawler`. Owner commands are printed, never run by the router; full 40-char sha, absolute `cd`. Spawn: `router-agent` worker-ops template with `-n trawler-<area>-<phase> --model claude-opus-5-5 --permission-mode auto`.

```sh
# Router: verify (read-only)
cd $WT && git branch --show-current && git rev-parse HEAD && git status --short | head -3
git merge-base --is-ancestor epic/<slug> HEAD && echo based-on-epic
for r in test doc cr design mr; do echo "$r: $(git notes --ref=$r show HEAD 2>&1 | head -1)"; done
cargo xtask verify-dod wu HEAD --target epic/<slug>     # only 8·manual-review red
actionlint .github/workflows/*.yml; echo actionlint=$?  # when workflows changed
git -C $M/Trawler log --oneline -3 main; git -C $M/Trawler rev-parse main origin/main; git tag | tail -3
# Owner: rule-bearing doc (router first checks `git diff --cached --name-only`)
cd $WT && cargo xtask mr commit -m $'<E-CODE.Wn>: <subject>\n\n<body; apostrophes as \'>' --as vuluu
# Owner: WU (the epic branch must not be checked out in another worktree)
cd $WT && cargo xtask mr confirm wu <full tip sha> --as vuluu --target epic/<slug> --report docs/work/<date>-<epic>-w<n>-<slug>-result.html
cd $WT && git switch epic/<slug> && git merge --no-ff wu/<epic-slug>/<slug>
# Owner: epic
cd $WT && cargo xtask mr confirm epic <full tip sha> --as vuluu --target main --report docs/.archive/<date>-<slug>-epic-result.html
cd $M/Trawler && git merge --no-ff epic/<slug>
cd $M/Trawler && git push origin main
# Owner: release tag (version must equal the tag without "v"); CI
cd $WT && grep -m1 '^version' Cargo.toml
cd $WT && git tag v<version> <full tip sha | epic/<slug>> && git push origin v<version>
cd $WT && git push origin <branch> && gh workflow run ci.yml --ref <branch>
# Router: watch (one background command; filter by sha)
until id=$(gh run list --workflow ci.yml --branch <br> --limit 1 --json databaseId,status,headSha -q '.[0] | select(.status=="completed" and (.headSha|startswith("<sha8>"))) | .databaseId') && [ -n "$id" ]; do sleep 45; done; echo "RUN $id"; gh run view $id --json conclusion,jobs -q '.jobs[]|"\(.name): \(.conclusion)"'
gh run list --workflow release.yml -L 3 --json databaseId,headBranch,status,conclusion -q '.[]|"\(.databaseId) \(.headBranch) \(.status) \(.conclusion)"'
# Worker: a parallel WU worktree
git -C $M/Trawler worktree add ../Trawler-<short> -b wu/<epic-slug>/<slug> epic/<slug>
# Router: retire a closed worktree (merged, clean, no process; keep the branch: notes live on it)
cd $M && b=$(git -C Trawler-<slug> branch --show-current); git -C Trawler merge-base --is-ancestor $b main && echo merged
git -C Trawler-<slug> status --short | wc -l; ps -axo pid,command | grep 'Trawler-<slug>' | grep -v grep
cp -Rn Trawler-<slug>/docs/.archive/. Trawler/docs/.archive/
for f in $(cd Trawler-<slug>/docs/.archive && find . -type f); do cmp -s "Trawler-<slug>/docs/.archive/$f" "Trawler/docs/.archive/$f" || echo "DIFF $f"; done
git -C Trawler worktree remove ../Trawler-<slug> && git -C Trawler worktree prune
# Owner try-recipe guards
mv ~/Library/Application\ Support/dev.trawler.app/keep-capturing.json ~/kc.bak   # before; reverse after
lsof -nP -iTCP:7420 -iTCP:7421 -iTCP:4445 -sTCP:LISTEN                               # quit other Trawlers first
```
