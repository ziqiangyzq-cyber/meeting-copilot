#!/usr/bin/env node
// Cross-platform dispatcher: picks the right OS-specific bundle script
// so Tauri's `beforeBuildCommand` works on both macOS and Windows.
import { execSync } from 'node:child_process';
import { platform } from 'node:os';

const os = platform();
if (os === 'darwin') {
  console.log('[bundle-helper] macOS detected, running bash script');
  execSync('bash src-tauri/scripts/bundle-audio-helper.sh', { stdio: 'inherit' });
} else if (os === 'win32') {
  console.log('[bundle-helper] Windows detected, running PowerShell script');
  execSync(
    'powershell -ExecutionPolicy Bypass -File src-tauri/scripts/bundle-audio-helper.ps1',
    { stdio: 'inherit' }
  );
} else {
  throw new Error(`unsupported platform: ${os}`);
}
