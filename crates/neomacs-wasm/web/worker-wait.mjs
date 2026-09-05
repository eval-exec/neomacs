// Values shared with the raw worker's HostWake enum.
export const HostWake = Object.freeze({ Input: 1, TimedOut: 2, Ready: 3 });

export class WorkerWait {
  constructor(hasInput) {
    this.hasInput = hasInput;
    this.pending = null;
  }

  wait(milliseconds) {
    if (this.hasInput()) return Promise.resolve(HostWake.Input);
    return new Promise(resolve => {
      const timer = setTimeout(() => {
        this.pending = null;
        resolve(HostWake.TimedOut);
      }, milliseconds);
      this.pending = reason => {
        clearTimeout(timer);
        this.pending = null;
        resolve(this.hasInput() ? HostWake.Input : reason);
      };
    });
  }

  notify(reason = HostWake.Ready) {
    this.pending?.(reason);
  }
}
