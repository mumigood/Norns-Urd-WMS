export type CameraFrame = { source_name?: string; source_time?: string };
type Latest = CameraFrame & { ok: boolean; fresh?: boolean };
const identity = (frame: CameraFrame) => frame.source_time ? `${frame.source_time}/${frame.source_name ?? ""}` : null;

/** Poll the server's camera inbox; never compare server timestamps with the browser clock. */
export class CameraWatch {
  private seen = new Set<string>();
  private busy = false;
  async poll(latest: () => Promise<Latest>, capture: () => Promise<CameraFrame | void>, active: () => boolean) {
    if (this.busy || !active()) return;
    this.busy = true;
    try {
      const frame = await latest();
      const key = identity(frame);
      if (!active() || !frame.ok || !frame.fresh || !key || this.seen.has(key)) return;
      const result = await capture();
      if (!result) return;
      this.seen.add(key);
      const captured = identity(result);
      if (captured) this.seen.add(captured);
    } finally {
      this.busy = false;
    }
  }
}
