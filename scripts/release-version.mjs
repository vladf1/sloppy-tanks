import { readFileSync } from "node:fs";

/** The release version players and `/health` see: `MAJOR.MINOR.PATCH.BUILD`, such as
 * `1.1.0.628`. The first three parts are `version` in package.json, bumped by hand; the
 * build is the Pages workflow's run number, which only main's deploys pass. Pull request
 * and local builds have no build number and report the three parts alone.
 *
 * The build number orders releases, so it is a version part rather than SemVer build
 * metadata (`1.1.0+628`), which SemVer ignores when comparing versions. */
export function releaseVersion(buildNumber = "") {
  const { version } = JSON.parse(readFileSync(new URL("../package.json", import.meta.url), "utf8"));
  if (!/^\d+\.\d+\.\d+$/.test(version)) {
    throw new Error(`package.json version must be MAJOR.MINOR.PATCH, not ${version}`);
  }
  if (buildNumber && !/^[1-9]\d*$/.test(buildNumber)) {
    throw new Error(`The build number must be a positive integer, not ${buildNumber}`);
  }
  return buildNumber ? `${version}.${buildNumber}` : version;
}

/** Whether a release carries a CI build number (four parts). */
export function isNumberedRelease(release = "") {
  return /^\d+\.\d+\.\d+\.\d+$/.test(release);
}
