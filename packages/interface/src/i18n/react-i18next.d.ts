import 'react-i18next';
import type { Resources } from './types';

declare module 'react-i18next' {
  interface CustomTypeOptions {
    resources: Resources;
  }
}
