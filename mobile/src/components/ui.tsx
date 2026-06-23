import { CheckCircle2, Moon, Sun, XCircle } from 'lucide-react-native';
import type { ReactNode } from 'react';
import { ActivityIndicator, Pressable, Text, View } from 'react-native';

import { useTheme } from '../theme';

export function ScreenHeader({
  eyebrow,
  left,
  right,
  title,
}: {
  eyebrow?: string;
  left?: ReactNode;
  right?: ReactNode;
  title: string;
}) {
  const { styles } = useTheme();

  return (
    <View style={styles.header}>
      <View style={styles.headerTop}>
        {left ?? <View style={styles.headerSide} />}
        <View style={styles.headerTextWrap}>
          <Text numberOfLines={1} style={styles.headerTitle}>
            {title}
          </Text>
          {eyebrow && (
            <Text numberOfLines={1} style={styles.headerEyebrow}>
              {eyebrow}
            </Text>
          )}
        </View>
        <View style={styles.headerSide}>
          <View style={styles.headerActions}>
            {right}
            <ThemeToggle />
          </View>
        </View>
      </View>
    </View>
  );
}

function ThemeToggle() {
  const { colors, mode, styles, toggleMode } = useTheme();
  const isDark = mode === 'dark';

  return (
    <IconButton
      accessibilityLabel={isDark ? 'Use light mode' : 'Use dark mode'}
      icon={
        isDark ? (
          <Sun color={colors.text} size={20} />
        ) : (
          <Moon color={colors.text} size={20} />
        )
      }
      onPress={toggleMode}
      style={styles.themeToggle}
    />
  );
}

export function AppButton({
  compact = false,
  disabled = false,
  icon,
  label,
  onPress,
  variant = 'primary',
}: {
  compact?: boolean;
  disabled?: boolean;
  icon?: ReactNode;
  label: string;
  onPress: () => void;
  variant?: 'primary' | 'secondary' | 'danger';
}) {
  const { styles } = useTheme();

  return (
    <Pressable
      disabled={disabled}
      onPress={onPress}
      style={({ pressed }) => [
        styles.button,
        compact && styles.buttonCompact,
        variant === 'secondary' && styles.buttonSecondary,
        variant === 'danger' && styles.buttonDanger,
        disabled && styles.buttonDisabled,
        pressed && !disabled && styles.pressed,
      ]}
    >
      {icon}
      <Text
        numberOfLines={1}
        style={[
          styles.buttonText,
          variant === 'secondary' && styles.buttonSecondaryText,
          variant === 'danger' && styles.buttonDangerText,
          disabled && styles.buttonDisabledText,
        ]}
      >
        {label}
      </Text>
    </Pressable>
  );
}

export function IconButton({
  accessibilityLabel,
  icon,
  onPress,
  style,
}: {
  accessibilityLabel: string;
  icon: ReactNode;
  onPress: () => void;
  style?: object;
}) {
  const { styles } = useTheme();

  return (
    <Pressable
      accessibilityLabel={accessibilityLabel}
      onPress={onPress}
      style={({ pressed }) => [
        styles.iconButton,
        style,
        pressed && styles.pressed,
      ]}
    >
      {icon}
    </Pressable>
  );
}

export function SegmentButton({
  active,
  icon,
  label,
  onPress,
}: {
  active: boolean;
  icon: ReactNode;
  label: string;
  onPress: () => void;
}) {
  const { styles } = useTheme();

  return (
    <Pressable
      onPress={onPress}
      style={({ pressed }) => [
        styles.segmentButton,
        active && styles.segmentButtonActive,
        pressed && styles.pressed,
      ]}
    >
      {icon}
      <Text
        numberOfLines={1}
        style={[
          styles.segmentText,
          active && styles.segmentTextActive,
        ]}
      >
        {label}
      </Text>
    </Pressable>
  );
}

export function Chip({ label, onPress }: { label: string; onPress: () => void }) {
  const { styles } = useTheme();

  return (
    <Pressable
      onPress={onPress}
      style={({ pressed }) => [styles.chip, pressed && styles.pressed]}
    >
      <Text style={styles.chipText}>{label}</Text>
    </Pressable>
  );
}

export function InlineNotice({
  text,
  tone,
}: {
  text: string;
  tone: 'error' | 'success';
}) {
  const { colors, styles } = useTheme();

  return (
    <View
      style={[
        styles.notice,
        tone === 'error' ? styles.noticeError : styles.noticeSuccess,
      ]}
    >
      {tone === 'error' ? (
        <XCircle color={colors.danger} size={18} />
      ) : (
        <CheckCircle2 color={colors.success} size={18} />
      )}
      <Text
        style={[
          styles.noticeText,
          tone === 'error' ? styles.noticeErrorText : styles.noticeSuccessText,
        ]}
      >
        {text}
      </Text>
    </View>
  );
}

export function LoadingBlock({ label }: { label: string }) {
  const { colors, styles } = useTheme();

  return (
    <View style={styles.loadingBlock}>
      <ActivityIndicator color={colors.accent} />
      <Text style={styles.emptyText}>{label}</Text>
    </View>
  );
}

export function EmptyState({ title }: { title: string }) {
  const { styles } = useTheme();

  return (
    <View style={styles.emptyPanel}>
      <Text style={styles.emptyTitle}>{title}</Text>
    </View>
  );
}
