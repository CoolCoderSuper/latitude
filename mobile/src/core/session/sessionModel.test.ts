import {
  mergeSessions,
  parseSessions,
  reorderSessions,
  sanitizeSession,
  sessionLabel,
  upsertSession,
} from './sessionModel';

const first = { baseUrl: 'http://first:8080', token: 'one' };
const second = { baseUrl: 'http://second:8080', token: 'two' };

describe('sessionModel', () => {
  it('sanitizes persisted session fields', () => {
    expect(
      sanitizeSession({
        baseUrl: ' http://latitude:8080/// ',
        token: ' token ',
        deviceHostname: ' workstation ',
      }),
    ).toEqual({
      baseUrl: 'http://latitude:8080',
      token: 'token',
      deviceHostname: 'workstation',
    });
    expect(sanitizeSession({ baseUrl: '', token: 'token' })).toBeNull();
  });

  it('recovers valid sessions from malformed persisted lists', () => {
    expect(parseSessions('not json')).toEqual([]);
    expect(
      parseSessions(
        JSON.stringify([first, { bad: true }, { ...first, token: 'new' }]),
      ),
    ).toEqual([{ ...first, token: 'new' }]);
  });

  it('preserves ordering while merging, replacing, and reordering', () => {
    expect(mergeSessions([first, second, { ...first, token: 'new' }])).toEqual([
      { ...first, token: 'new' },
      second,
    ]);
    expect(upsertSession([first, second], { ...first, token: 'new' })).toEqual([
      { ...first, token: 'new' },
      second,
    ]);
    expect(reorderSessions([first, second], 0, 1)).toEqual([second, first]);
  });

  it('prefers a saved hostname for display', () => {
    expect(sessionLabel({ ...first, deviceHostname: 'latitude-box' })).toBe(
      'latitude-box',
    );
    expect(sessionLabel(first)).toBe('first:8080');
  });
});
