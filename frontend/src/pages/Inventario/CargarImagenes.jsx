import { useRef, useState } from 'react';
import { api } from '../../api/api';
import { comprimirImagen } from '../../utils/comprimirImagen';
import './ImportarProductos.css';

// Cargar muchas fotos de productos de una vez (solo el administrador). Cada
// archivo se nombra con el código del producto ("7753749007955.jpg"); se
// emparejan solas, se achican en el navegador y se suben de a una (el
// servidor procesa una imagen a la vez, ver logica/imagen.rs). Usa la misma
// ruta que la foto de la ficha del producto: nada nuevo en el servidor.

const EXTENSIONES = /\.(jpe?g|png|webp|gif)$/i;
const MAXIMO_ARCHIVOS = 2000;

/** "7753749007955.jpg" o "7753749007955 - Cusqueña.png" -> posibles códigos. */
function codigosDe(nombreArchivo) {
  const base = nombreArchivo.replace(/\.[^.]+$/, '').trim();
  const primero = base.split(/[\s_]+/)[0];
  return [...new Set([base, primero].map((c) => c.toLowerCase()).filter(Boolean))];
}

/** Con el módulo Catálogo: "7753749007955_2.jpg" es la foto 2 de ese código. */
function codigosFoto2(nombreArchivo) {
  const base = nombreArchivo.replace(/\.[^.]+$/, '').trim();
  const m = base.match(/^(.+?)_2$/);
  return m ? codigosDe(m[1]) : [];
}

export default function CargarImagenes({ productos, onCerrar, onTerminado, conFoto2 = false, fotos2 = new Map() }) {
  const [emparejados, setEmparejados] = useState(null);
  const [sinProducto, setSinProducto] = useState([]);
  const [reemplazar, setReemplazar] = useState(false);
  const [subiendo, setSubiendo] = useState(false);
  const [hechos, setHechos] = useState(0);
  const [errores, setErrores] = useState([]);
  const [error, setError] = useState(null);
  const detener = useRef(false);

  const elegir = (lista) => {
    setError(null);
    const archivos = [...(lista || [])].filter((a) => EXTENSIONES.test(a.name));
    if (archivos.length === 0) {
      setError('Elige fotos JPG, PNG o WEBP nombradas con el código del producto.');
      return;
    }
    if (archivos.length > MAXIMO_ARCHIVOS) {
      setError(`Máximo ${MAXIMO_ARCHIVOS} fotos por vez.`);
      return;
    }
    const porCodigo = new Map(productos.map((p) => [String(p.codigo).trim().toLowerCase(), p]));
    const ok = [];
    const sin = [];
    const usados = new Set();
    for (const archivo of archivos) {
      // Si el nombre completo es un código, es su foto 1 (como siempre). Si no
      // y termina en _2 (con el módulo Catálogo), es la foto 2 de ese código.
      const base = archivo.name.replace(/\.[^.]+$/, '').trim().toLowerCase();
      const delCodigo = (lista) => lista.map((c) => porCodigo.get(c)).find(Boolean);
      let cual = 1;
      let producto = porCodigo.get(base);
      if (!producto && conFoto2) {
        producto = delCodigo(codigosFoto2(archivo.name));
        if (producto) cual = 2;
      }
      if (!producto) producto = delCodigo(codigosDe(archivo.name));
      const clave = `${producto?.id}-${cual}`;
      if (!producto) sin.push(archivo.name);
      else if (usados.has(clave)) sin.push(`${archivo.name} (otra foto ${cual} del mismo producto)`);
      else {
        usados.add(clave);
        ok.push({ archivo, producto, cual });
      }
    }
    setEmparejados(ok);
    setSinProducto(sin);
    setHechos(0);
    setErrores([]);
  };

  const yaTiene = (e) => (e.cual === 2 ? fotos2.has(e.producto.id) : Boolean(e.producto.imagen_url));
  const aSubir = (emparejados || []).filter((e) => reemplazar || !yaTiene(e));
  const conFoto = (emparejados || []).filter(yaTiene).length;

  const subir = async () => {
    setSubiendo(true);
    detener.current = false;
    const fallas = [];
    let n = 0;
    for (const { archivo, producto, cual } of aSubir) {
      if (detener.current) break;
      try {
        const comprimida = await comprimirImagen(archivo);
        if (cual === 2) await api.productoSubirFoto2(producto.id, comprimida);
        else await api.productoSubirImagen(producto.id, comprimida);
      } catch (e) {
        fallas.push(`${producto.nombre}: ${e.message}`);
      }
      n += 1;
      setHechos(n);
      setErrores([...fallas]);
    }
    setSubiendo(false);
    onTerminado({ subidas: n - fallas.length, errores: fallas.length, detenido: detener.current });
  };

  const porcentaje = aSubir.length ? Math.round((hechos / aSubir.length) * 100) : 0;

  return (
    <div className="inv-modal-overlay" onClick={subiendo ? undefined : onCerrar}>
      <div className="inv-modal imp-modal" onClick={(e) => e.stopPropagation()} role="dialog" aria-label="Cargar fotos de productos">
        <h2>Cargar fotos de productos</h2>
        {!emparejados ? (
          <>
            <p className="imp-nota">
              Elige todas las fotos de una vez. Cada una debe llamarse como el <strong>código del producto</strong> (por ejemplo{' '}
              <code>7753749007955.jpg</code>). Se achican solas para que no pesen.
              {conFoto2 && (
                <>
                  {' '}
                  Para la <strong>foto 2</strong> agrega <code>_2</code> al final: <code>7753749007955_2.jpg</code>.
                </>
              )}
            </p>
            <label className="imp-zona">
              <input type="file" accept="image/jpeg,image/png,image/webp,image/gif" multiple onChange={(e) => elegir(e.target.files)} />
              <strong>Elige las fotos</strong>
              <span>JPG, PNG o WEBP · hasta {MAXIMO_ARCHIVOS} a la vez</span>
            </label>
            {error && <p className="inv-mensaje inv-mensaje-error">{error}</p>}
            <div className="inv-modal-acciones">
              <button className="inv-boton-cancelar" onClick={onCerrar}>
                Cancelar
              </button>
            </div>
          </>
        ) : (
          <>
            <p className="imp-nota">
              <strong>{emparejados.length}</strong> foto{emparejados.length === 1 ? '' : 's'} con su producto
              {conFoto > 0 && (
                <>
                  {' '}
                  (<strong>{conFoto}</strong> de esos productos ya tiene foto)
                </>
              )}
              {sinProducto.length > 0 && (
                <>
                  {' '}
                  · <strong>{sinProducto.length}</strong> sin producto con ese código (no se suben)
                </>
              )}
              .
            </p>
            {conFoto > 0 && (
              <label className="imp-nota">
                <input type="checkbox" checked={reemplazar} disabled={subiendo} onChange={(e) => setReemplazar(e.target.checked)} /> Reemplazar
                también las fotos que ya tienen
              </label>
            )}
            {sinProducto.length > 0 && (
              <details className="imp-nota">
                <summary>Ver las que no tienen producto</summary>
                <ul>
                  {sinProducto.slice(0, 200).map((n) => (
                    <li key={n}>{n}</li>
                  ))}
                </ul>
              </details>
            )}
            {(subiendo || hechos > 0) && (
              <p className="imp-nota">
                Subiendo {hechos} de {aSubir.length} ({porcentaje}%)…{' '}
                {!subiendo && hechos === aSubir.length ? 'listo.' : 'no cierres esta ventana.'}
              </p>
            )}
            {errores.length > 0 && (
              <details className="inv-mensaje inv-mensaje-error" open>
                <summary>{errores.length} no se pudieron subir</summary>
                <ul>
                  {errores.slice(0, 50).map((e) => (
                    <li key={e}>{e}</li>
                  ))}
                </ul>
              </details>
            )}
            <div className="inv-modal-acciones">
              {subiendo ? (
                <button className="inv-boton-cancelar" onClick={() => (detener.current = true)}>
                  Detener
                </button>
              ) : (
                <button className="inv-boton-cancelar" onClick={() => setEmparejados(null)}>
                  Elegir otras fotos
                </button>
              )}
              <button className="inv-boton-guardar" onClick={subir} disabled={subiendo || aSubir.length === 0}>
                {subiendo ? 'Subiendo…' : `Subir ${aSubir.length} foto${aSubir.length === 1 ? '' : 's'}`}
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
