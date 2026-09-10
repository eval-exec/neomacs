// One in-flight input batch. Both acceptance and rejection retire it; only
// acceptance promises that the evaluator received the validated events.
export class WorkerInput {
  #pending = null;
  #post;

  constructor(post) {
    this.#post = post;
  }

  enqueue(batch) {
    if (this.#pending !== null) throw new Error("browser input is already pending");
    this.#pending = {
      bytes: new TextEncoder().encode(JSON.stringify(batch)),
      sequence: typeof batch?.sequence === "string" ? batch.sequence : null,
    };
  }

  bytes() {
    return this.#pending?.bytes ?? null;
  }

  accept(sequence) {
    if (this.#pending === null || this.#pending.sequence !== sequence) return false;
    this.#pending = null;
    this.#post({ type: "input-accepted", sequence });
    return true;
  }

  reject(message) {
    if (this.#pending === null) return false;
    const { sequence } = this.#pending;
    this.#pending = null;
    this.#post({ type: "input-rejected", sequence, message });
    return true;
  }
}
