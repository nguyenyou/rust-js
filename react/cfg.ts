// The rustc flags that build the react crate for one React version (ADR 0043):
// `--cfg react="18.1"` and so on for every minor release up to it, so an
// item gated `#[cfg(react = "19.2")]` exists only from React 19.2.
//
//   bun react/cfg.ts 18.2.0    prints one flag per line, for react/build.sh
//
// The releases are react/versions.json's, which react/generate.ts reads from
// React itself.

import versions from "./versions.json" with { type: "json" };

export const releases: string[] = Object.keys(versions.releases);
export const latest = releases.at(-1)!;

function minor(version: string): number[] {
  const [major, minor] = version.split(".").map(Number);
  return [major, minor];
}

function atMost(a: number[], b: number[]): boolean {
  return a[0] < b[0] || (a[0] === b[0] && a[1] <= b[1]);
}

/**
 * The flags for `version`, and a warning when it's newer than the releases
 * the crate knows, which then gets the latest's API. Throws when it's older
 * than the first release the crate supports.
 */
export function cfgFlags(version: string): { flags: string[]; warning?: string } {
  const wanted = minor(version);
  if (wanted.some(Number.isNaN)) throw new Error(`not a React version: ${version}`);
  if (!atMost(minor(releases[0]), wanted)) {
    throw new Error(`React ${version} is older than ${releases[0]}, the first release the react crate supports`);
  }
  const values = releases.map((r) => JSON.stringify(r)).join(",");
  const flags = [`--check-cfg=cfg(react,values(${values}))`];
  for (const release of releases) {
    if (atMost(minor(release), wanted)) flags.push(`--cfg=react=${JSON.stringify(release)}`);
  }
  const warning = atMost(wanted, minor(latest))
    ? undefined
    : `React ${version} is newer than ${latest}, the latest release the react crate knows: it has ${latest}'s API`;
  return { flags, warning };
}

if (import.meta.main) {
  try {
    const { flags, warning } = cfgFlags(process.argv[2] ?? latest);
    if (warning) console.error(`warning: ${warning}`);
    console.log(flags.join("\n"));
  } catch (error) {
    console.error(`error: ${(error as Error).message}`);
    process.exit(1);
  }
}
