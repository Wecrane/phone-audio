# Security policy

## Reporting a vulnerability

Please **do not** open a public issue for a security problem.

Use GitHub's private vulnerability reporting if it is enabled for this
repository, or contact the maintainer through the GitHub profile linked in the
repository metadata. Include:

- affected version / commit;
- reproduction steps;
- impact (e.g. command execution, local privilege boundary, data exposure);
- any suggested fix.

## Scope

Phone Audio runs as a local desktop application and drives `adb` / `scrcpy`.
Relevant security issues include, but are not limited to:

- command injection through adb/scrcpy arguments or device serials;
- unsafe handling of paths / registry values;
- unexpected network exposure (the app itself does not listen on a socket);
- supply-chain issues in downloads or the patched scrcpy build.

The Android device's USB/wireless debugging authorization remains the user's
responsibility; the app never bypasses adb authorization or Android security
boundaries.
