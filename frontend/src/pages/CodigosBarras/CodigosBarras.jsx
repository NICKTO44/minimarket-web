import { useEffect, useMemo, useState } from 'react';
import { Barcode, Copy, Download, Printer, RefreshCw, Trash2 } from 'lucide-react';
import { api } from '../../api/api';
import { imprimirHtml } from '../../utils/facturaA4';
import { descargarArchivo } from '../../utils/pdfTicket';
import { escribirXlsx, TIPO_XLSX } from '../../utils/hojas';
import {
  PAPELES,
  TIPOS,
  borrarHistorial,
  cssEtiquetas,
  documentoParaImprimir,
  errorDePrefijo,
  etiquetasPorHoja,
  generarTanda,
  guardarHistorial,
  htmlEtiquetas,
  leerHistorial,
  papelDe,
} from '../../utils/codigosBarras';
import '../../components/PantallaModulo.css';
import './CodigosBarras.css';

// Módulo "Códigos de barras" (CODIGOS_BARRAS): tandas de códigos aleatorios
// para imprimir en etiquetas. Solo es del navegador: lee los productos para
// no repetir un código que ya existe, pero no guarda nada en el servidor.
// El historial de lo generado queda en este equipo (utils/codigosBarras.js).

const CANTIDADES = [25, 50, 100];

// La última tanda y las opciones, en memoria mientras no se recargue la
// página: al ir a otra pantalla y volver, siguen ahí.
let memoria = null;

export default function CodigosBarras({ nombreTienda = '', identificadorNegocio }) {
  const negocio = identificadorNegocio || nombreTienda;
  const inicial = memoria?.negocio === negocio ? memoria : null;

  const [cantidad, setCantidad] = useState(inicial?.cantidad ?? 50);
  const [tipo, setTipo] = useState(inicial?.tipo ?? 'EAN13');
  const [prefijo, setPrefijo] = useState(inicial?.prefijo ?? TIPOS.EAN13.prefijo);
  const [largo, setLargo] = useState(inicial?.largo ?? 8);
  const [copias, setCopias] = useState(inicial?.copias ?? 1);
  const [texto, setTexto] = useState(inicial?.texto ?? '');
  const [papel, setPapel] = useState(inicial?.papel ?? 'A4_4');
  const [borde, setBorde] = useState(inicial?.borde ?? false);
  const [tanda, setTanda] = useState(inicial?.tanda ?? null); // { numero, tipo, codigos }

  const [codigosProductos, setCodigosProductos] = useState(null); // Set o null mientras carga
  const [avisoProductos, setAvisoProductos] = useState('');
  const [mensaje, setMensaje] = useState(null); // { tipo: 'exito' | 'error', texto }
  const [confirmarBorrado, setConfirmarBorrado] = useState(false);
  const [enHistorial, setEnHistorial] = useState(() => leerHistorial(negocio).codigos.length);

  useEffect(() => {
    memoria = { negocio, cantidad, tipo, prefijo, largo, copias, texto, papel, borde, tanda };
  }, [negocio, cantidad, tipo, prefijo, largo, copias, texto, papel, borde, tanda]);

  // Códigos que ya usan los productos (activos y desactivados): no se repiten.
  useEffect(() => {
    let vivo = true;
    Promise.allSettled([api.productos(), api.productosDesactivados()]).then(([activos, desactivados]) => {
      if (!vivo) return;
      const codigos = new Set();
      for (const r of [activos, desactivados]) {
        if (r.status === 'fulfilled' && Array.isArray(r.value)) {
          for (const p of r.value) if (p?.codigo) codigos.add(String(p.codigo).trim().toUpperCase());
        }
      }
      setCodigosProductos(codigos);
      if (activos.status === 'rejected') {
        setAvisoProductos('No se pudieron leer tus productos: los códigos nuevos solo se comparan con los ya generados en este equipo.');
      }
    });
    return () => {
      vivo = false;
    };
  }, []);

  const cambiarTipo = (nuevo) => {
    setTipo(nuevo);
    setPrefijo(TIPOS[nuevo].prefijo);
  };

  const generar = () => {
    setMensaje(null);
    const limpio = prefijo.trim().toUpperCase();
    const error = errorDePrefijo(tipo, limpio);
    if (error) return setMensaje({ tipo: 'error', texto: error });
    const n = Math.max(1, Math.min(1000, Number.parseInt(cantidad, 10) || 0));
    const historial = leerHistorial(negocio);
    const ocupados = new Set([...(codigosProductos || []), ...historial.codigos]);
    const codigos = generarTanda({ tipo, prefijo: limpio, largo: Math.max(4, Math.min(16, Number(largo) || 8)), cantidad: n, ocupados });
    if (!codigos) {
      return setMensaje({ tipo: 'error', texto: 'Con ese prefijo ya no quedan suficientes códigos libres. Usa un prefijo más corto o distinto.' });
    }
    const numero = historial.tanda + 1;
    guardarHistorial(negocio, { codigos: [...historial.codigos, ...codigos], tanda: numero });
    setEnHistorial(historial.codigos.length + codigos.length);
    setPrefijo(limpio);
    setCantidad(n);
    setTanda({ numero, tipo, codigos });
    setMensaje({ tipo: 'exito', texto: `Tanda ${numero}: ${codigos.length} códigos nuevos, ninguno repetido.` });
  };

  const papelElegido = papelDe(papel);
  const nCopias = Math.max(1, Math.min(20, Number.parseInt(copias, 10) || 1));
  const opcionesEtiqueta = { copias: nCopias, texto: texto.trim(), borde };

  const vistaPrevia = useMemo(
    () => (tanda ? htmlEtiquetas({ codigos: tanda.codigos, tipo: tanda.tipo, ...opcionesEtiqueta }) : ''),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [tanda, nCopias, texto, borde],
  );

  const imprimir = () => {
    if (!tanda) return;
    imprimirHtml(documentoParaImprimir({ papel: papelElegido, codigos: tanda.codigos, tipo: tanda.tipo, ...opcionesEtiqueta }));
  };

  const descargarExcel = () => {
    if (!tanda) return;
    // Mismos títulos que lee Importar productos; el código va como texto.
    const filas = [['Código', 'Nombre', 'Precio de venta'], ...tanda.codigos.map((c) => [c, '', ''])];
    descargarArchivo(escribirXlsx(filas, { anchos: [18, 40, 16] }), `Codigos-de-barras-tanda-${tanda.numero}.xlsx`, { tipo: TIPO_XLSX });
  };

  const copiar = async () => {
    if (!tanda) return;
    try {
      await navigator.clipboard.writeText(tanda.codigos.join('\n'));
      setMensaje({ tipo: 'exito', texto: 'Lista copiada.' });
    } catch {
      setMensaje({ tipo: 'error', texto: 'No se pudo copiar. Usa "Descargar Excel".' });
    }
  };

  const borrar = () => {
    borrarHistorial(negocio);
    setEnHistorial(0);
    setConfirmarBorrado(false);
    setMensaje({ tipo: 'exito', texto: 'Historial borrado. Los códigos de tus productos se siguen respetando.' });
  };

  const totalEtiquetas = tanda ? tanda.codigos.length * nCopias : 0;
  const porHoja = etiquetasPorHoja(papelElegido);

  return (
    <div className="pm-layout cb-layout">
      <div className="pm-cabecera">
        <div>
          <h1>
            <Barcode size={22} /> Códigos de barras
          </h1>
          <p className="pm-subtitulo">Genera tandas de códigos que no se repiten y los imprime en etiquetas. No se guarda nada en el sistema.</p>
        </div>
      </div>

      {avisoProductos && <p className="pm-mensaje pm-mensaje-aviso">{avisoProductos}</p>}

      <section className="pm-tarjeta cb-panel">
        <div className="cb-campos">
          <label className="pm-campo">
            <span>Cantidad de códigos</span>
            <div className="cb-cantidad">
              <input type="number" min="1" max="1000" value={cantidad} onChange={(e) => setCantidad(e.target.value)} />
              <div className="pm-filtros">
                {CANTIDADES.map((n) => (
                  <button key={n} type="button" className={Number(cantidad) === n ? 'activo' : ''} onClick={() => setCantidad(n)}>
                    {n}
                  </button>
                ))}
              </div>
            </div>
          </label>

          <label className="pm-campo">
            <span>Tipo de código</span>
            <select value={tipo} onChange={(e) => cambiarTipo(e.target.value)}>
              {Object.entries(TIPOS).map(([valor, t]) => (
                <option key={valor} value={valor}>
                  {t.label}
                </option>
              ))}
            </select>
          </label>

          <label className="pm-campo cb-corto">
            <span>Prefijo</span>
            <input value={prefijo} maxLength={tipo === 'EAN13' ? 4 : 6} onChange={(e) => setPrefijo(e.target.value.toUpperCase())} />
          </label>

          {tipo === 'CODE128' && (
            <label className="pm-campo cb-corto">
              <span>Largo (sin prefijo)</span>
              <input type="number" min="4" max="16" value={largo} onChange={(e) => setLargo(e.target.value)} />
            </label>
          )}

          <label className="pm-campo cb-corto">
            <span>Copias de cada código</span>
            <input type="number" min="1" max="20" value={copias} onChange={(e) => setCopias(e.target.value)} />
          </label>

          <label className="pm-campo">
            <span>Texto arriba (opcional)</span>
            <input value={texto} maxLength={40} placeholder={nombreTienda || 'Nombre de tu tienda'} onChange={(e) => setTexto(e.target.value)} />
          </label>

          <label className="pm-campo">
            <span>Dónde imprimes</span>
            <select value={papel} onChange={(e) => setPapel(e.target.value)}>
              {PAPELES.map((p) => (
                <option key={p.valor} value={p.valor}>
                  {p.label}
                </option>
              ))}
            </select>
          </label>

          <label className="cb-check">
            <input type="checkbox" checked={borde} onChange={(e) => setBorde(e.target.checked)} /> Borde para recortar
          </label>
        </div>

        <p className="cb-nota">
          {tipo === 'EAN13'
            ? 'EAN-13: 13 números con dígito de control. Los prefijos 20 a 29 son de uso interno de tiendas, así no chocan con los códigos de fábrica. Lo lee cualquier lector.'
            : 'Code 128: letras y números (sin I, O, 0 ni 1 para no confundirlos). Si tu lector escribe mal las letras, revisa el idioma del teclado del lector o usa EAN-13.'}
        </p>

        <div className="pm-acciones">
          <button type="button" className="pm-boton" onClick={generar} disabled={codigosProductos === null}>
            <RefreshCw size={16} /> {codigosProductos === null ? 'Leyendo productos…' : 'Generar tanda'}
          </button>
          <button type="button" className="pm-boton" onClick={imprimir} disabled={!tanda}>
            <Printer size={16} /> Imprimir
          </button>
          <button type="button" className="pm-boton-secundario" onClick={descargarExcel} disabled={!tanda}>
            <Download size={16} /> Descargar Excel
          </button>
          <button type="button" className="pm-boton-secundario" onClick={copiar} disabled={!tanda}>
            <Copy size={16} /> Copiar lista
          </button>
          {enHistorial > 0 && !confirmarBorrado && (
            <button type="button" className="pm-boton-secundario" onClick={() => setConfirmarBorrado(true)}>
              <Trash2 size={16} /> Borrar historial
            </button>
          )}
        </div>

        {confirmarBorrado && (
          <div className="pm-mensaje pm-mensaje-aviso cb-confirmar">
            <span>
              ¿Borrar el historial de {enHistorial} códigos generados en este equipo? Después podría salir uno que ya imprimiste y aún no usaste.
            </span>
            <button type="button" className="pm-boton-peligro" onClick={borrar}>
              Sí, borrar
            </button>
            <button type="button" className="pm-boton-secundario" onClick={() => setConfirmarBorrado(false)}>
              No
            </button>
          </div>
        )}

        {mensaje && <p className={`pm-mensaje pm-mensaje-${mensaje.tipo} cb-mensaje`}>{mensaje.texto}</p>}
      </section>

      {tanda ? (
        <>
          <div className="pm-resumen">
            <div className="pm-dato">
              <span>Tanda</span>
              <strong>{tanda.numero}</strong>
            </div>
            <div className="pm-dato">
              <span>Códigos</span>
              <strong>{tanda.codigos.length}</strong>
            </div>
            <div className="pm-dato">
              <span>Etiquetas</span>
              <strong>{totalEtiquetas}</strong>
            </div>
            {porHoja && (
              <div className="pm-dato">
                <span>Hojas A4</span>
                <strong>{Math.ceil(totalEtiquetas / porHoja)}</strong>
              </div>
            )}
          </div>
          <div className="pm-tarjeta cb-vista">
            <style>{cssEtiquetas(papelElegido)}</style>
            <div className="ce-hoja" dangerouslySetInnerHTML={{ __html: vistaPrevia }} />
          </div>
        </>
      ) : (
        <div className="pm-tarjeta">
          <p className="pm-vacio">
            Elige cuántos códigos necesitas y pulsa <b>Generar tanda</b>.
            {enHistorial > 0 && ` Ya generaste ${enHistorial} en este equipo; no se repetirán.`}
          </p>
        </div>
      )}
    </div>
  );
}
