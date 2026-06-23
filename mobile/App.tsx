import 'react-native-gesture-handler';

import { AppContent } from './src/AppContent';
import { ThemeProvider } from './src/theme';

export default function App() {
  return (
    <ThemeProvider>
      <AppContent />
    </ThemeProvider>
  );
}
