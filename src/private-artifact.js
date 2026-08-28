import { chmod, mkdir, stat } from "node:fs/promises";
import { dirname } from "node:path";

export async function preparePrivateArtifact(filePath) {
  const artifactDirectory = dirname(filePath);
  await mkdir(artifactDirectory, { recursive: true, mode: 0o700 });
  const directoryMode = (await stat(artifactDirectory)).mode & 0o777;
  if (directoryMode !== 0o700) {
    throw new Error(
      `Private artifact directory must use mode 0700; received ${directoryMode.toString(8)}`,
    );
  }
  try {
    await enforcePrivateFile(filePath);
  } catch (error) {
    if (error?.code !== "ENOENT") {
      throw error;
    }
  }
}

export async function enforcePrivateFile(filePath) {
  await chmod(filePath, 0o600);
}
