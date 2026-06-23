import { StatusBar } from 'expo-status-bar';
import { Server } from 'lucide-react-native';
import type { ReactNode } from 'react';
import { ActivityIndicator, Text, View } from 'react-native';
import { SafeAreaProvider, SafeAreaView } from 'react-native-safe-area-context';

import { useTheme } from '../theme';

export function Shell({ children }: { children: ReactNode }) {
  const { mode, styles } = useTheme();

  return (
    <SafeAreaProvider>
      <SafeAreaView style={styles.safeArea}>
        <StatusBar style={mode === 'dark' ? 'light' : 'dark'} />
        {children}
      </SafeAreaView>
    </SafeAreaProvider>
  );
}

export function LoadingScreen() {
  const { colors, styles } = useTheme();

  return (
    <View style={styles.centered}>
      <Server color={colors.accent} size={34} />
      <Text style={styles.loadingTitle}>Latitude</Text>
      <ActivityIndicator color={colors.accent} />
    </View>
  );
}
