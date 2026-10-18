import { COMPATIBILIDAD_VACIA } from '../../utils/repuestos';

// Secciones "Ficha del repuesto" (módulo FICHA_REPUESTO) y "Vehículos
// compatibles" (módulo COMPATIBILIDAD) del formulario de producto. Todo es
// opcional: un producto sin nada escrito aquí es un producto común.
// `marcas`: marcas de repuesto ya usadas; `vehiculos`: [{ marca, modelos }]
// ya cargados, para sugerir al escribir.
export default function FichaRepuesto({
  conFicha = false,
  conCompatibilidad = false,
  campos,
  onCambiar,
  filas = [],
  onCambiarFilas,
  marcas = [],
  vehiculos = [],
}) {
  const cambiar = (campo, valor) => onCambiar({ ...campos, [campo]: valor });
  const cambiarFila = (i, campo, valor) => onCambiarFilas(filas.map((f, j) => (j === i ? { ...f, [campo]: valor } : f)));
  const modelosDe = (marca) =>
    vehiculos.find((v) => v.marca.toLowerCase() === String(marca || '').trim().toLowerCase())?.modelos || [];

  return (
    <>
      {conFicha && (
        <div className="inv-ficha-caja">
          <p className="inv-lote-titulo">Ficha del repuesto</p>
          <p className="inv-lote-nota">
            Opcional. El punto de venta encuentra el producto por cualquiera de sus códigos, y los productos que comparten
            un código se ofrecen como equivalentes.
          </p>
          <div className="inv-ficha-campos">
            <div className="inv-campo">
              <label>Marca</label>
              <input
                value={campos.marca}
                maxLength={60}
                list="inv-marcas-repuesto"
                placeholder="Bosch, Castrol, Wix…"
                onChange={(e) => cambiar('marca', e.target.value)}
              />
              <datalist id="inv-marcas-repuesto">
                {marcas.map((m) => (
                  <option key={m} value={m} />
                ))}
              </datalist>
            </div>
            <div className="inv-campo">
              <label>N.° de parte / código del fabricante</label>
              <input
                value={campos.numero_parte}
                maxLength={40}
                placeholder="90915-YZZE1"
                onChange={(e) => cambiar('numero_parte', e.target.value.toUpperCase())}
              />
            </div>
            <div className="inv-campo inv-ficha-ancho">
              <label>Códigos alternos (equivalencias)</label>
              <input
                value={campos.codigos}
                placeholder="PH3614 (Fram), W712/75 (Mann), 51348 (Wix)"
                onChange={(e) => cambiar('codigos', e.target.value)}
              />
              <small className="inv-ficha-ayuda">
                Separa con comas. El fabricante entre paréntesis es opcional. Se busca con o sin guiones ni espacios.
              </small>
            </div>
            <div className="inv-campo">
              <label>Ubicación en el almacén</label>
              <input
                value={campos.ubicacion}
                maxLength={60}
                placeholder="Estante 3 - B"
                onChange={(e) => cambiar('ubicacion', e.target.value)}
              />
            </div>
            <div className="inv-campo">
              <label>Garantía (días)</label>
              <input
                type="number"
                min="0"
                max="3650"
                value={campos.garantia_dias}
                placeholder="Sin garantía"
                onChange={(e) => cambiar('garantia_dias', e.target.value)}
              />
              <small className="inv-ficha-ayuda">Sale en el ticket de venta (30 = 1 mes, 365 = 1 año).</small>
            </div>
          </div>
        </div>
      )}

      {conCompatibilidad && (
        <div className="inv-ficha-caja">
          <p className="inv-lote-titulo">Vehículos compatibles</p>
          <p className="inv-lote-nota">
            Opcional. Sin modelo = sirve para toda la marca; sin años = todos los años. En el punto de venta se busca por
            vehículo.
          </p>
          <datalist id="inv-marcas-vehiculo">
            {vehiculos.map((v) => (
              <option key={v.marca} value={v.marca} />
            ))}
          </datalist>
          {filas.map((f, i) => (
            <div key={i} className="inv-compat-fila">
              <input
                className="inv-compat-marca"
                value={f.marca}
                maxLength={40}
                list="inv-marcas-vehiculo"
                placeholder="Marca"
                aria-label="Marca del vehículo"
                onChange={(e) => cambiarFila(i, 'marca', e.target.value)}
              />
              <input
                className="inv-compat-modelo"
                value={f.modelo}
                maxLength={60}
                list={`inv-modelos-${i}`}
                placeholder="Modelo"
                aria-label="Modelo"
                onChange={(e) => cambiarFila(i, 'modelo', e.target.value)}
              />
              <datalist id={`inv-modelos-${i}`}>
                {modelosDe(f.marca).map((m) => (
                  <option key={m} value={m} />
                ))}
              </datalist>
              <input
                className="inv-compat-anio"
                type="number"
                value={f.anio_desde}
                placeholder="Desde"
                aria-label="Año desde"
                onChange={(e) => cambiarFila(i, 'anio_desde', e.target.value)}
              />
              <input
                className="inv-compat-anio"
                type="number"
                value={f.anio_hasta}
                placeholder="Hasta"
                aria-label="Año hasta"
                onChange={(e) => cambiarFila(i, 'anio_hasta', e.target.value)}
              />
              <input
                className="inv-compat-motor"
                value={f.motor}
                maxLength={40}
                placeholder="Motor (opcional)"
                aria-label="Motor"
                onChange={(e) => cambiarFila(i, 'motor', e.target.value)}
              />
              <button
                type="button"
                className="inv-compat-quitar"
                aria-label="Quitar este vehículo"
                onClick={() => onCambiarFilas(filas.filter((_, j) => j !== i))}
              >
                ✕
              </button>
            </div>
          ))}
          <button
            type="button"
            className="inv-compat-agregar"
            onClick={() => onCambiarFilas([...filas, { ...COMPATIBILIDAD_VACIA, marca: filas.at(-1)?.marca || '' }])}
          >
            + Agregar vehículo
          </button>
        </div>
      )}
    </>
  );
}
