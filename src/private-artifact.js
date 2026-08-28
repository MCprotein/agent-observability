import { chmod, mkdir } from "node:fs/promises";
import { dirname } from "node:path";

export async function preparePrivateArtifact(filePath) {
  await mkdir(dirname(filePath), { recursive: true, mode: 0o700 });
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
