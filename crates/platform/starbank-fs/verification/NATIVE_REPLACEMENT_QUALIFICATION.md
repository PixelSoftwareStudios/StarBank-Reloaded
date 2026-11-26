# Native replacement qualification for WP-07 and VO-05

Baseline `dfe70d8`. Linux replacement is an explicit WP-07 follow-up. The tested evidence below supports a narrow Linux syscall proof, but leaves Linux and Windows replacement unavailable in the product.

## Linux ext4 evidence

The pinned Rust image `rust:1.100.0-slim-bookworm` at `sha256:bb875b0f8ce726053e259034848c471840e96bfe591fd52556178ae9a05e6126` ran a native C probe using Linux 6.12.76-linuxkit arm64. The disposable Docker volume was mounted as ext4 according to Linux mountinfo. The container had zero effective and permitted capabilities, no network and no host filesystem bind mounts. The source probe used only synthetic bank strings and ACL entries.

- `renameat2(RENAME_EXCHANGE)` installed the staged inode and preserved the displaced inode and old bytes as a sibling. Both file sync and directory sync returned success. Read-back confirmed both contents, mode, uid, gid, one named-user POSIX ACL and a user xattr. Ownership was equal in the fixture, so owner reassignment was outside this test.
- Setting a new `trusted.*` test xattr failed with `EPERM`. The probe did not seed a privileged attribute. The ext4 list implementation itself gates visibility on `CAP_SYS_ADMIN`, and Linux documents the same ability boundary. A normal reader cannot tell whether an existing file has an invisible trusted attribute. [Linux ext4 xattr handler](https://codebrowser.dev/linux/linux/fs/ext4/xattr_trusted.c.html#ext4_xattr_trusted_list) [Linux xattr namespaces](https://man7.org/linux/man-pages/man7/xattr.7.html)
- A retained original inode can keep metadata for recovery, while the live path now carries the staged inode. Comparing readable ACLs and attributes proves visible parity only. This cannot establish the no-silent-metadata-change invariant for an arbitrary existing ext4 bank. A safe candidate needs filesystem-proven absence of hidden metadata or a kernel operation that preserves it with qualified crash behavior. The editor should keep default refusal until such a path qualifies. [Linux rename exchange semantics](https://man7.org/linux/man-pages/man2/rename.2.html#DESCRIPTION)

Reproduction using the accompanying synthetic probe:

```sh
docker run --rm -i --pull=never --network none --read-only \
  --cap-drop ALL --security-opt no-new-privileges \
  --tmpfs /tmp:rw,noexec,nosuid,nodev \
  --mount type=volume,destination=/probe,volume-nocopy \
  rust:1.100.0-slim-bookworm \
  sh -c 'set -eu
    cc -std=c17 -Wall -Wextra -Werror -x c - -o /probe/probe
    /probe/probe' < ext4-visible-probe.c
```

This establishes native Linux behavior on the disposable local ext4 volume with the stated fixtures. No product Linux overwrite path, power interruption, privileged-attribute copying, fscrypt, fs-verity, project IDs, network filesystem or overlay mount was exercised.

## Windows NTFS authoring and verifier gaps

The existing product reads local Windows snapshots and cross-compiles the Win32 metadata bindings. There is no native Windows runtime evidence for this baseline. The Mac host lacks the MSVC linker, and the available virtual-machine CLI could not reach its local daemon. Start with an ordinary local NTFS fixture directory on a native Windows runner and an unprivileged product token. Keep create-only and replace-qualified capabilities separate.

| Boundary | Prerequisite before enabling a writer |
|---|---|
| Scope | Hold the validated parent handles through publication and verify the actual target volume and filesystem. Restrict qualification to local NTFS. Reject symlink/junction/reparse, offline, read-only, non-regular and multiply linked targets and unqualified file attributes before creating a stage. Exercise a concurrent parent or name change. |
| Create only | Stage exclusively under a private security descriptor in the existing destination directory. Flush the staged file, publish without the replace or cross-volume copy modes, then verify the destination has its expected file ID and digest. A file that appears just before publication must survive byte-for-byte. |
| Replacing an expected file | Capture volume and 128-bit file ID, bytes, and supported metadata through no-follow handles. Keep a same-volume recovery name that cannot collide with a user file. Test `ReplaceFileW` with zero flags, stage and target sharing masks, and an original-name swap immediately before publication. Verify the destination is the stage inode and the displaced name is the observed original. The snapshot reader currently denies write and delete sharing. A replacement probe needs a deliberately narrower sharing contract alongside explicit late-conflict detection. |
| Metadata verifier | Measure and compare owner, group, DACL, descriptor flags and supported file attributes. Use a fixture with explicit and inherited ACEs. Independently identify SACL behavior, security resource attributes, encryption/compression and named data streams. A candidate may reject unqualified files before publication when it can identify them. Failure to read a SACL does not establish that it is absent, and the product should not enable additional token privileges. |
| Durability | Distinguish file flush, parent namespace publication, read-back and power survival. `ReplaceFileW` does not support its write-through flag. Qualify available unprivileged flush behavior and name persistence separately. Process termination before replacement, after replacement and after verification provides recovery evidence while the OS remains running. Power survival needs separate evidence. |
| Outcomes | Preserve Windows last-error immediately. Treat a failed post-publication check or ambiguous namespace error as uncertain, preserve both candidate artifacts and require explicit recovery. Microsoft documents 1175 with the original names intact, 1176 with names depending on the requested backup, and 1177 with a moved original and possible metadata merge. Re-observe names, identities and bytes before an unchanged disposition. |

Microsoft documents that `ReplaceFileW` preserves selected fields, including DACLs and named streams subject to merge rules, and publishes the stage's file ID. That preservation list does not establish owner, group and full audit ACL parity. `GetSecurityInfo` needs `READ_CONTROL` for the owner, group and DACL and `ACCESS_SYSTEM_SECURITY` with the proper privilege for a SACL. A handle-relative stream information query or an independently bound enumerator must account for metadata not present in the unnamed bank stream. [Microsoft ReplaceFileW](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-replacefilew) [Microsoft GetSecurityInfo](https://learn.microsoft.com/en-us/windows/win32/api/aclapi/nf-aclapi-getsecurityinfo) [Microsoft stream enumeration](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-findfirststreamw) [Microsoft FlushFileBuffers](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-flushfilebuffers)

Initial native acceptance should cover a normal destination, a competing create, sharing masks that permit and refuse replacement, a late external change, each observable partial-failure state, named data streams, explicit and inherited DACLs, an inaccessible descriptor, link and reparse refusal, and process death. The verifier must use independently captured identity and metadata for the synthetic inputs and record the Windows build, volume type, token capabilities, exit codes and case results.
