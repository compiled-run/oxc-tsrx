export const NATIVE_TARGETS = Object.freeze([
  Object.freeze({
    target: "aarch64-apple-darwin",
    packageSuffix: "darwin-arm64",
    os: "darwin",
    cpu: "arm64",
    vscodeTarget: "darwin-arm64",
  }),
  Object.freeze({
    target: "x86_64-apple-darwin",
    packageSuffix: "darwin-x64",
    os: "darwin",
    cpu: "x64",
    vscodeTarget: "darwin-x64",
  }),
  Object.freeze({
    target: "aarch64-unknown-linux-gnu",
    packageSuffix: "linux-arm64-gnu",
    os: "linux",
    cpu: "arm64",
    libc: "glibc",
    vscodeTarget: "linux-arm64",
  }),
  Object.freeze({
    target: "x86_64-unknown-linux-gnu",
    packageSuffix: "linux-x64-gnu",
    os: "linux",
    cpu: "x64",
    libc: "glibc",
    vscodeTarget: "linux-x64",
  }),
  Object.freeze({
    target: "aarch64-unknown-linux-musl",
    packageSuffix: "linux-arm64-musl",
    os: "linux",
    cpu: "arm64",
    libc: "musl",
    vscodeTarget: "alpine-arm64",
  }),
  Object.freeze({
    target: "x86_64-unknown-linux-musl",
    packageSuffix: "linux-x64-musl",
    os: "linux",
    cpu: "x64",
    libc: "musl",
    vscodeTarget: "alpine-x64",
  }),
  Object.freeze({
    target: "aarch64-pc-windows-msvc",
    packageSuffix: "win32-arm64-msvc",
    os: "win32",
    cpu: "arm64",
    vscodeTarget: "win32-arm64",
  }),
  Object.freeze({
    target: "x86_64-pc-windows-msvc",
    packageSuffix: "win32-x64-msvc",
    os: "win32",
    cpu: "x64",
    vscodeTarget: "win32-x64",
  }),
]);

export function nativePackageName(target) {
  return `@oxc-tsrx/native-${target.packageSuffix}`;
}

export function nativeTargetForHost(os, cpu, libc = undefined) {
  const target = NATIVE_TARGETS.find(
    (candidate) =>
      candidate.os === os &&
      candidate.cpu === cpu &&
      (candidate.os !== "linux" || candidate.libc === libc),
  );
  if (target === undefined) {
    const identity = `${os}-${cpu}${libc ? `-${libc}` : ""}`;
    throw new RangeError(`OXC for TSRX has no native package for ${identity}`);
  }
  return target;
}
