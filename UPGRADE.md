# Applying this source package to your existing Git repository

The ZIP is a **source overlay**, not a replacement for your tested Git history. It deliberately excludes `.git`, `Cargo.lock`, and `pixi.lock`. Keep the lockfiles already generated and committed on your Ubuntu machine, then regenerate `Cargo.lock` for the new workspace members.

1. Extract the ZIP anywhere **outside** `~/Projects/pixi-fleetforge`, for example `~/Downloads/fleetforge-upgrade`.
2. From your existing repo root, create a feature branch:

   ```bash
   cd ~/Projects/pixi-fleetforge
   git switch -c feature/fleet-control-plane
   ```

3. Overlay the extracted contents without touching `.git`:

   ```bash
   cp -a ~/Downloads/fleetforge-upgrade/pixi-fleetforge/. ~/Projects/pixi-fleetforge/
   ```

4. Install updated dependencies and validate:

   ```bash
   cd ~/Projects/pixi-fleetforge
   pixi install
   pixi run cargo fmt --all
   pixi run clippy
   pixi run test
   pixi run demo
   pixi run demo-full
   pixi run demo-peer
   ```

5. If the core checks pass, run dashboard checks:

   ```bash
   cd dashboard
   npm install
   npm run check
   npm run build
   cd ..
   ```

6. Review changes, commit on the feature branch and push **only after checks pass**:

   ```bash
   git status
   git diff --stat
   git add -A
   git commit -m "Add local fleet control plane and deployment lifecycle"
   git push -u origin feature/fleet-control-plane
   gh pr create --base main --head feature/fleet-control-plane
   ```

The optional ROS 2 and libp2p research environments use independent dependency files and need separate checks. See their READMEs. Do not merge if new Rust checks fail; send the compiler output for a targeted fix.

**Important:** the downloaded package has not been compiled in the generation environment. v0.1 was compiled and tested by you; the new v0.2 code is an implementation candidate until you complete the above checks.
