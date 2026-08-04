import { StyleSheet } from 'react-native';

export const viewerStyles = StyleSheet.create({
  viewer: { flex: 1, backgroundColor: '#050505' },
  media: { flex: 1, width: '100%', backgroundColor: '#050505' },
  statusOverlay: {
    ...StyleSheet.absoluteFillObject,
    alignItems: 'center',
    justifyContent: 'center',
    gap: 10,
    padding: 20,
    backgroundColor: 'rgba(5, 5, 5, 0.72)',
  },
  statusText: {
    color: '#f8fafc',
    fontSize: 14,
    fontWeight: '800',
    textAlign: 'center',
  },
  errorText: {
    color: '#fecaca',
    fontSize: 14,
    fontWeight: '800',
    textAlign: 'center',
  },
});
