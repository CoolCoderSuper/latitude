import {
  FileText,
  Film,
  FolderOpen,
  Globe2,
  Image as ImageIcon,
} from 'lucide-react-native';

import type { ThemeColors } from '../../theme';
import type { DeploymentSummary } from '../../types';
import { isImageMediaType, isVideoMediaType } from './media';

export function deploymentIcon(deployment: DeploymentSummary, colors: ThemeColors) {
  if (isVideoMediaType(deployment.media_type)) {
    return <Film color={colors.coral} size={21} />;
  }
  if (isImageMediaType(deployment.media_type)) {
    return <ImageIcon color={colors.gold} size={21} />;
  }

  switch (deployment.kind) {
    case 'reverse_proxy':
      return <Globe2 color={colors.accent} size={21} />;
    case 'static':
      return <FolderOpen color={colors.gold} size={21} />;
    case 'page':
      return <FileText color={colors.coral} size={21} />;
  }
}
