import { defineConfig } from 'vite'

import svg from '@poppanator/sveltekit-svg'
import { sveltekit } from '@sveltejs/kit/vite'

import viteSvgToWebfont from 'vite-svg-2-webfont'
import { resolve } from 'path'
import { existsSync, mkdirSync } from 'fs'
import SvgFixer from 'oslllo-svg-fixer'

export default defineConfig(async () => {
  if (!existsSync('tmp/assets/icons/feldera-icons-fixed')) {
    mkdirSync('tmp/assets/icons/feldera-icons-fixed', { recursive: true })
    await SvgFixer('src/assets/icons/feldera-icons', 'tmp/assets/icons/feldera-icons-fixed').fix()
  }
  return {
    plugins: [
      sveltekit(),
      svg(),
      viteSvgToWebfont({
        context: resolve(__dirname, 'tmp/assets/icons/feldera-icons-fixed'),
        fontName: 'FelderaIconsFont',
        baseSelector: '.fd',
        classPrefix: 'fd-',
        moduleId: 'vite-svg-2-webfont.css',
        cssFontsUrl: '/'
      })
    ],
    build: {
      minify: false
    }
  }
})
