// "Ver" de Comprobantes para la factura en hoja A4 propia (emisión directa):
// muestra en pantalla la misma hoja que sale al imprimir
// (htmlComprobanteA4 de utils/facturaA4.js), achicada para que entre en el
// ancho de la ventana, con botones para imprimirla o cerrar. Usa los estilos
// del visor de PDF de Comprobantes (comp-pdf-modal-*).
import { useEffect, useRef, useState } from 'react';

// Ancho de una hoja A4 en pantalla (210 mm a 96 ppp).
const ANCHO_A4 = 794;

export default function VistaComprobanteHtml({ html, titulo, onCerrar }) {
  const cajaRef = useRef(null);
  const marcoRef = useRef(null);
  const [escala, setEscala] = useState(1);
  const [alto, setAlto] = useState(1123);

  // Achica la hoja al ancho disponible y toma el alto real de su contenido.
  const ajustar = () => {
    const ancho = cajaRef.current?.clientWidth || ANCHO_A4;
    // 24 px de aire a los lados, como las páginas del visor de PDF.
    setEscala(Math.min(1, (ancho - 24) / ANCHO_A4));
    const doc = marcoRef.current?.contentDocument;
    if (doc?.documentElement) setAlto(Math.max(doc.documentElement.scrollHeight, 400));
  };

  useEffect(() => {
    window.addEventListener('resize', ajustar);
    const tecla = (e) => e.key === 'Escape' && onCerrar();
    window.addEventListener('keydown', tecla);
    return () => {
      window.removeEventListener('resize', ajustar);
      window.removeEventListener('keydown', tecla);
    };
  }, [onCerrar]);

  const imprimir = () => {
    const ventana = marcoRef.current?.contentWindow;
    if (!ventana) return;
    ventana.focus();
    ventana.print();
  };

  return (
    <div className="comp-pdf-modal-overlay">
      <div className="comp-pdf-modal">
        <div className="comp-pdf-modal-header comp-no-imprimir">
          <h2>{titulo}</h2>
          <button type="button" className="comp-pdf-modal-x" onClick={onCerrar} aria-label="Cerrar">
            ×
          </button>
        </div>

        <div className="comp-pdf-paginas" ref={cajaRef}>
          {/* La caja mide lo mismo que la hoja ya achicada: así el visor la
              centra bien y no queda cortada a un lado. */}
          <div style={{ width: ANCHO_A4 * escala, height: alto * escala, overflow: 'hidden', margin: '0 auto', flex: 'none' }}>
            <iframe
              ref={marcoRef}
              title={titulo}
              srcDoc={html}
              onLoad={ajustar}
              style={{
                display: 'block',
                width: ANCHO_A4,
                height: alto,
                border: 0,
                background: '#fff',
                transform: `scale(${escala})`,
                transformOrigin: '0 0',
              }}
            />
          </div>
        </div>

        <div className="comp-pdf-modal-acciones comp-no-imprimir">
          <button className="comp-pdf-modal-cerrar" onClick={onCerrar}>
            Cerrar
          </button>
          <button className="comp-pdf-modal-imprimir" onClick={imprimir}>
            🖨 Imprimir
          </button>
        </div>
      </div>
    </div>
  );
}
