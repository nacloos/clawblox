import React, { CSSProperties } from 'react'
import { GuiElement as GuiElementType, UDim2 } from '../api'

interface GuiElementProps {
  element: GuiElementType
  onGuiClick?: (elementId: number) => void
}

/**
 * Convert UDim2 to CSS calc() expression
 */
function udim2ToCSS(udim: UDim2 | undefined, property: 'left' | 'top' | 'width' | 'height'): string {
  if (!udim) {
    return property === 'width' || property === 'height' ? '100px' : '0px'
  }

  const isX = property === 'left' || property === 'width'
  const scale = isX ? udim.x_scale : udim.y_scale
  const offset = isX ? udim.x_offset : udim.y_offset

  if (scale === 0) {
    return `${offset}px`
  } else if (offset === 0) {
    return `${scale * 100}%`
  } else {
    return `calc(${scale * 100}% + ${offset}px)`
  }
}

/**
 * Convert Color3 array [r, g, b] (0-1) to CSS color
 */
function colorToCSS(color: [number, number, number] | undefined): string {
  if (!color) return 'transparent'
  const [r, g, b] = color
  return `rgb(${Math.round(r * 255)}, ${Math.round(g * 255)}, ${Math.round(b * 255)})`
}

/**
 * Get CSS text alignment
 */
function textAlignToCSS(alignment: 'Left' | 'Center' | 'Right' | undefined): CSSProperties['textAlign'] {
  switch (alignment) {
    case 'Left':
      return 'left'
    case 'Right':
      return 'right'
    default:
      return 'center'
  }
}

/**
 * Get CSS align-items for vertical alignment
 */
function textYAlignToCSS(alignment: 'Top' | 'Center' | 'Bottom' | undefined): CSSProperties['alignItems'] {
  switch (alignment) {
    case 'Top':
      return 'flex-start'
    case 'Bottom':
      return 'flex-end'
    default:
      return 'center'
  }
}

export default function GuiElement({ element, onGuiClick }: GuiElementProps) {
  // Skip invisible elements
  if (element.visible === false) {
    return null
  }

  // ScreenGui is just a container, render children directly
  if (element.type === 'ScreenGui') {
    if (element.enabled === false) {
      return null
    }
    return (
      <>
        {element.children.map((child) => (
          <GuiElement key={child.id} element={child} onGuiClick={onGuiClick} />
        ))}
      </>
    )
  }

  // Calculate anchor point offset
  const anchorX = element.anchor_point?.[0] ?? 0
  const anchorY = element.anchor_point?.[1] ?? 0

  // Build style
  const style: CSSProperties = {
    position: 'absolute',
    left: udim2ToCSS(element.position, 'left'),
    top: udim2ToCSS(element.position, 'top'),
    width: udim2ToCSS(element.size, 'width'),
    height: udim2ToCSS(element.size, 'height'),
    transform: `translate(${-anchorX * 100}%, ${-anchorY * 100}%)`,
    zIndex: element.z_index ?? 1,
    boxSizing: 'border-box',
    overflow: 'hidden',
  }

  // Rotation
  if (element.rotation && element.rotation !== 0) {
    style.transform += ` rotate(${element.rotation}deg)`
  }

  // Background
  const bgTransparency = element.background_transparency ?? 0
  if (bgTransparency < 1) {
    style.backgroundColor = colorToCSS(element.background_color)
    style.opacity = 1 - bgTransparency
  }

  // Border
  if (element.border_size_pixel && element.border_size_pixel > 0) {
    style.border = `${element.border_size_pixel}px solid ${colorToCSS(element.border_color)}`
  }

  // Text styling for TextLabel/TextButton
  const hasText = element.text !== undefined && element.text !== null
  if (hasText) {
    style.display = 'flex'
    style.justifyContent = textAlignToCSS(element.text_x_alignment)
    style.alignItems = textYAlignToCSS(element.text_y_alignment)
    style.color = colorToCSS(element.text_color)
    style.fontSize = element.text_size ? `${element.text_size}px` : '14px'
    style.fontFamily = 'sans-serif'
    if (element.text_transparency !== undefined) {
      // Combine with background transparency
      const textOpacity = 1 - element.text_transparency
      style.color = style.color?.replace('rgb', 'rgba').replace(')', `, ${textOpacity})`)
    }
  }

  // Handle click for buttons
  const isButton = element.type === 'TextButton' || element.type === 'ImageButton'
  const handleClick = isButton && onGuiClick
    ? (e: React.MouseEvent) => {
        e.stopPropagation()
        onGuiClick(element.id)
      }
    : undefined

  // Button hover effect
  if (isButton) {
    style.cursor = 'pointer'
  }

  // Render based on type
  const renderContent = () => {
    switch (element.type) {
      case 'Frame':
        return null
      case 'TextLabel':
      case 'TextButton':
        return <span>{element.text}</span>
      case 'ImageLabel':
      case 'ImageButton':
        if (element.image) {
          return (
            <img
              src={element.image}
              alt=""
              style={{
                width: '100%',
                height: '100%',
                objectFit: 'contain',
                opacity: element.image_transparency !== undefined ? 1 - element.image_transparency : 1,
              }}
            />
          )
        }
        return null
      default:
        return null
    }
  }

  return (
    <div style={style} onClick={handleClick}>
      {renderContent()}
      {element.children.map((child) => (
        <GuiElement key={child.id} element={child} onGuiClick={onGuiClick} />
      ))}
    </div>
  )
}
