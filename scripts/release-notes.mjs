import fs from "node:fs";

const changelog = fs.readFileSync("CHANGELOG.md", "utf8").replace(/^\uFEFF/, "");
const heading = /^##\s+(.+?)\r?$/m.exec(changelog);

if (!heading) {
  throw new Error("CHANGELOG.md must contain at least one level-two version entry");
}

const bodyStart = heading.index + heading[0].length;
const remaining = changelog.slice(bodyStart);
const nextHeading = /^##\s+/m.exec(remaining);
const body = (nextHeading ? remaining.slice(0, nextHeading.index) : remaining).trim();

if (!body) {
  throw new Error("The first CHANGELOG.md version entry must contain release notes");
}

process.stdout.write(`## ${heading[1].trim()}\n\n${body}\n`);
