export class LatestRequest {
  #controller = null;
  #version = 0;

  begin() {
    this.#controller?.abort();
    this.#controller = new AbortController();
    this.#version += 1;
    return { controller: this.#controller, version: this.#version };
  }

  isCurrent(version) {
    return version === this.#version;
  }

  finish(version) {
    if (this.isCurrent(version)) this.#controller = null;
  }
}
