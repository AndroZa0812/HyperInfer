const { execSync } = require('child_process');
const fs = require('fs');
const path = require('path');

try {
  // Get all files changed in the last commit (or PR merge)
  // For merge commits, we want the diff against the first parent (HEAD^1)
  const diffOutput = execSync('git diff --name-only HEAD^1 HEAD').toString();
  const changedFiles = diffOutput.trim().split('\n');
  
  // Filter for package.json files not in the root
  const changedPackageJsons = changedFiles.filter(f => f.endsWith('package.json') && f !== 'package.json');

  const changedPackages = [];

  for (const pkgFile of changedPackageJsons) {
    const fullPath = path.join(process.cwd(), pkgFile);
    if (!fs.existsSync(fullPath)) continue;

    // Get the old version from HEAD^1 (before the merge/commit)
    let oldVersion = '';
    try {
      const oldContent = execSync(`git show HEAD^1:${pkgFile}`).toString();
      oldVersion = JSON.parse(oldContent).version;
    } catch (e) {
      // File might not exist in the old commit
    }

    // Get the new version from the current file
    const newContent = fs.readFileSync(fullPath, 'utf8');
    const pkgData = JSON.parse(newContent);
    const newVersion = pkgData.version;

    // If the version string changed, this package needs to be released
    if (newVersion && newVersion !== oldVersion) {
      changedPackages.push(pkgData.name);
    }
  }

  // Output as a valid JSON array for the GHA matrix strategy
  console.log(JSON.stringify(changedPackages));
} catch (error) {
  // If git fails or there's any other error, output empty array to skip release gracefully
  console.log(JSON.stringify([]));
}
