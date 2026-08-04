export type ActiveRequest = {
  controller: AbortController;
  id: number;
  key: string;
};

export class LatestRequestManager {
  private active: ActiveRequest | null = null;
  private nextId = 1;

  begin(key: string, dedupe = false): ActiveRequest | null {
    if (dedupe && this.active?.key === key) {
      return null;
    }

    this.cancel();
    const request = {
      controller: new AbortController(),
      id: this.nextId,
      key,
    };
    this.nextId += 1;
    this.active = request;
    return request;
  }

  isCurrent(request: ActiveRequest): boolean {
    return this.active === request;
  }

  finish(request: ActiveRequest): boolean {
    if (!this.isCurrent(request)) {
      return false;
    }

    this.active = null;
    return true;
  }

  cancel(): void {
    this.active?.controller.abort();
    this.active = null;
  }
}
