declare module 'react-cytoscapejs' {
  import { Component } from 'react';
  import type { Core, ElementDefinition, Stylesheet, LayoutOptions } from 'cytoscape';

  interface CytoscapeComponentProps {
    elements: ElementDefinition[];
    stylesheet?: Stylesheet[];
    style?: React.CSSProperties;
    layout?: LayoutOptions;
    cy?: (cy: Core) => void;
    pan?: { x: number; y: number };
    zoom?: number;
    minZoom?: number;
    maxZoom?: number;
    zoomingEnabled?: boolean;
    userZoomingEnabled?: boolean;
    panningEnabled?: boolean;
    userPanningEnabled?: boolean;
    boxSelectionEnabled?: boolean;
    autolock?: boolean;
    autoungrabify?: boolean;
    autounselectify?: boolean;
  }

  export default class CytoscapeComponent extends Component<CytoscapeComponentProps> {}
}
