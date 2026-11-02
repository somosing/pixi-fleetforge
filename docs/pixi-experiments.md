# Pixi evaluation log

This is a worksheet, **not** a claim that experiments have been completed.

## Reproducibility

1. Install Pixi: https://pixi.prefix.dev/latest/installation/
2. `pixi install`
3. `pixi run build`
4. `pixi run test`
5. Check how the generated `pixi.lock` behaves in a clean clone.

Questions: Are dependency errors actionable? Which changes cause lockfile regeneration? How clear is the offline behavior?

## Packaging

1. `pixi global install pixi-pack pixi-unpack`
2. `bash scripts/pack-robot-demo.sh`
3. Deploy `robot-demo/pixi-release.json` through the agent HTTP API.
4. Repeat after disabling network access (with packages already cached/packed).

Questions: What is the archive size? How long does unpack take? Which prerequisites remain on a minimal robot? Is the failure message useful when an environment is incomplete?

## ROS 2

Follow https://pixi.prefix.dev/latest/tutorials/ros2/ and record what happens when adding ROS 2 packages with RoboStack. Pay attention to `rosdep` workflow limitations; verify them against current documentation and actual commands before proposing an upstream change.

Record **observed output**, expected behavior, measured time, and a minimal reproduction. Don't call a limitation a bug without checking the current tool version and related issues.
