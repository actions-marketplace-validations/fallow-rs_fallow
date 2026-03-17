// Verify the correct platform-specific package was installed
function getPlatformPackage() {
  const platform = process.platform;
  const arch = process.arch;

  if (platform === 'win32' && arch === 'x64') {
    return '@fallow-cli/win32-x64-msvc';
  }
  if (platform === 'darwin') {
    return `@fallow-cli/darwin-${arch}`;
  }
  if (platform === 'linux') {
    try {
      const { familySync } = require('detect-libc');
      const libc = familySync() === 'musl' ? 'musl' : 'gnu';
      return `@fallow-cli/linux-${arch}-${libc}`;
    } catch {
      return `@fallow-cli/linux-${arch}-gnu`;
    }
  }

  return null;
}

const pkg = getPlatformPackage();

if (!pkg) {
  console.warn(
    `fallow: No prebuilt binary for ${process.platform}-${process.arch}. ` +
    `You can build from source: https://github.com/bartwaardenburg/fallow`
  );
  process.exit(0);
}

try {
  require.resolve(pkg);
} catch {
  console.warn(
    `fallow: Platform package ${pkg} not installed. ` +
    `This may happen if you used --no-optional. ` +
    `Run 'npm install' to fix.`
  );
}
