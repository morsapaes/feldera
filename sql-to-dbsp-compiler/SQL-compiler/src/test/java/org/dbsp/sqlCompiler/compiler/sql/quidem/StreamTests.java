package org.dbsp.sqlCompiler.compiler.sql.quidem;

import org.dbsp.sqlCompiler.compiler.DBSPCompiler;
import org.dbsp.sqlCompiler.compiler.sql.tools.SqlIoTest;
import org.junit.Test;

// Based on stream.iq from Calcite
public class StreamTests extends SqlIoTest {
    @Override
    public void prepareInputs(DBSPCompiler compiler) {
        String sql = """
                CREATE TABLE orders(
                   rowtime TIMESTAMP NOT NULL,
                   id      INTEGER,
                   product VARCHAR,
                   units   INTEGER
                );
                
                INSERT INTO orders VALUES
                ('2015-02-15 10:15:00', 1, 'paint', 10),
                ('2015-02-15 10:24:15', 2, 'paper', 5),
                ('2015-02-15 10:24:45', 3, 'brush', 12),
                ('2015-02-15 10:58:00', 4, 'paint', 3),
                ('2015-02-15 11:10:00', 5, 'paint', 3);""";
        compiler.compileStatements(sql);
    }

    @Test
    public void testNegativeTumble() {
        this.statementsFailingInCompilation("""
                CREATE VIEW V AS SELECT * FROM TABLE(
                  TUMBLE(
                    DATA => TABLE ORDERS,
                    TIMECOL => DESCRIPTOR(ROWTIME),
                    SIZE => INTERVAL -1 MINUTE))""",
                "Tumbling window interval must be positive");
    }

    @Test
    public void testTumble() {
        this.qs("""
                SELECT * FROM TABLE(
                  TUMBLE(
                    DATA => TABLE ORDERS,
                    TIMECOL => DESCRIPTOR(ROWTIME),
                    SIZE => INTERVAL '1' MINUTE));
                +---------------------+----+---------+-------+-------------------------+-------------------------+
                | ROWTIME             | ID | PRODUCT | UNITS | window_start            | window_end              |
                +---------------------+----+---------+-------+-------------------------+-------------------------+
                | 2015-02-15 10:15:00 |  1 | paint|       10 | 2015-02-15 10:15:00.000 | 2015-02-15 10:16:00.000 |
                | 2015-02-15 10:24:15 |  2 | paper|        5 | 2015-02-15 10:24:00.000 | 2015-02-15 10:25:00.000 |
                | 2015-02-15 10:24:45 |  3 | brush|       12 | 2015-02-15 10:24:00.000 | 2015-02-15 10:25:00.000 |
                | 2015-02-15 10:58:00 |  4 | paint|        3 | 2015-02-15 10:58:00.000 | 2015-02-15 10:59:00.000 |
                | 2015-02-15 11:10:00 |  5 | paint|        3 | 2015-02-15 11:10:00.000 | 2015-02-15 11:11:00.000 |
                +---------------------+----+---------+-------+-------------------------+-------------------------+
                (5 rows)
                
                SELECT * FROM TABLE(TUMBLE(TABLE ORDERS, DESCRIPTOR(ROWTIME), INTERVAL '1' MINUTE));
                +---------------------+----+---------+-------+-------------------------+-------------------------+
                | ROWTIME             | ID | PRODUCT | UNITS | window_start            | window_end              |
                +---------------------+----+---------+-------+-------------------------+-------------------------+
                | 2015-02-15 10:15:00 |  1 | paint|       10 | 2015-02-15 10:15:00.000 | 2015-02-15 10:16:00.000 |
                | 2015-02-15 10:24:15 |  2 | paper|        5 | 2015-02-15 10:24:00.000 | 2015-02-15 10:25:00.000 |
                | 2015-02-15 10:24:45 |  3 | brush|       12 | 2015-02-15 10:24:00.000 | 2015-02-15 10:25:00.000 |
                | 2015-02-15 10:58:00 |  4 | paint|        3 | 2015-02-15 10:58:00.000 | 2015-02-15 10:59:00.000 |
                | 2015-02-15 11:10:00 |  5 | paint|        3 | 2015-02-15 11:10:00.000 | 2015-02-15 11:11:00.000 |
                +---------------------+----+---------+-------+-------------------------+-------------------------+
                (5 rows)
                
                SELECT * FROM TABLE(TUMBLE((SELECT * FROM ORDERS), DESCRIPTOR(ROWTIME), INTERVAL '1' MINUTE));
                +---------------------+----+---------+-------+-------------------------+-------------------------+
                | ROWTIME             | ID | PRODUCT | UNITS | window_start            | window_end              |
                +---------------------+----+---------+-------+-------------------------+-------------------------+
                | 2015-02-15 10:15:00 |  1 | paint|       10 | 2015-02-15 10:15:00.000 | 2015-02-15 10:16:00.000 |
                | 2015-02-15 10:24:15 |  2 | paper|        5 | 2015-02-15 10:24:00.000 | 2015-02-15 10:25:00.000 |
                | 2015-02-15 10:24:45 |  3 | brush|       12 | 2015-02-15 10:24:00.000 | 2015-02-15 10:25:00.000 |
                | 2015-02-15 10:58:00 |  4 | paint|        3 | 2015-02-15 10:58:00.000 | 2015-02-15 10:59:00.000 |
                | 2015-02-15 11:10:00 |  5 | paint|        3 | 2015-02-15 11:10:00.000 | 2015-02-15 11:11:00.000 |
                +---------------------+----+---------+-------+-------------------------+-------------------------+
                (5 rows)
                
                SELECT * FROM TABLE(TUMBLE((SELECT * FROM ORDERS), DESCRIPTOR(ROWTIME), INTERVAL '10' MINUTE, INTERVAL '3' MINUTE));
                +---------------------+----+---------+-------+-------------------------+-------------------------+
                | ROWTIME             | ID | PRODUCT | UNITS | window_start            | window_end              |
                +---------------------+----+---------+-------+-------------------------+-------------------------+
                | 2015-02-15 10:15:00 |  1 | paint|       10 | 2015-02-15 10:13:00.000 | 2015-02-15 10:23:00.000 |
                | 2015-02-15 10:24:15 |  2 | paper|        5 | 2015-02-15 10:23:00.000 | 2015-02-15 10:33:00.000 |
                | 2015-02-15 10:24:45 |  3 | brush|       12 | 2015-02-15 10:23:00.000 | 2015-02-15 10:33:00.000 |
                | 2015-02-15 10:58:00 |  4 | paint|        3 | 2015-02-15 10:53:00.000 | 2015-02-15 11:03:00.000 |
                | 2015-02-15 11:10:00 |  5 | paint|        3 | 2015-02-15 11:03:00.000 | 2015-02-15 11:13:00.000 |
                +---------------------+----+---------+-------+-------------------------+-------------------------+
                (5 rows)""");
    }

    @Test
    public void testHop() {
        this.qs("""
                SELECT * FROM TABLE(HOP(TABLE ORDERS, DESCRIPTOR(ROWTIME), INTERVAL '5' MINUTE, INTERVAL '10' MINUTE));
                +---------------------+----+---------+-------+-------------------------+-------------------------+
                | ROWTIME             | ID | PRODUCT | UNITS | window_start            | window_end              |
                +---------------------+----+---------+-------+-------------------------+-------------------------+
                | 2015-02-15 10:15:00 |  1 | paint|       10 | 2015-02-15 10:10:00.000 | 2015-02-15 10:20:00.000 |
                | 2015-02-15 10:15:00 |  1 | paint|       10 | 2015-02-15 10:15:00.000 | 2015-02-15 10:25:00.000 |
                | 2015-02-15 10:24:15 |  2 | paper|        5 | 2015-02-15 10:15:00.000 | 2015-02-15 10:25:00.000 |
                | 2015-02-15 10:24:15 |  2 | paper|        5 | 2015-02-15 10:20:00.000 | 2015-02-15 10:30:00.000 |
                | 2015-02-15 10:24:45 |  3 | brush|       12 | 2015-02-15 10:15:00.000 | 2015-02-15 10:25:00.000 |
                | 2015-02-15 10:24:45 |  3 | brush|       12 | 2015-02-15 10:20:00.000 | 2015-02-15 10:30:00.000 |
                | 2015-02-15 10:58:00 |  4 | paint|        3 | 2015-02-15 10:50:00.000 | 2015-02-15 11:00:00.000 |
                | 2015-02-15 10:58:00 |  4 | paint|        3 | 2015-02-15 10:55:00.000 | 2015-02-15 11:05:00.000 |
                | 2015-02-15 11:10:00 |  5 | paint|        3 | 2015-02-15 11:05:00.000 | 2015-02-15 11:15:00.000 |
                | 2015-02-15 11:10:00 |  5 | paint|        3 | 2015-02-15 11:10:00.000 | 2015-02-15 11:20:00.000 |
                +---------------------+----+---------+-------+-------------------------+-------------------------+
                (10 rows)
                
                SELECT * FROM TABLE(
                  HOP(
                    DATA => TABLE ORDERS,
                    TIMECOL => DESCRIPTOR(ROWTIME),
                    SLIDE => INTERVAL '5' MINUTE,
                    SIZE => INTERVAL '10' MINUTE));
                +---------------------+----+---------+-------+-------------------------+-------------------------+
                | ROWTIME             | ID | PRODUCT | UNITS | window_start            | window_end              |
                +---------------------+----+---------+-------+-------------------------+-------------------------+
                | 2015-02-15 10:15:00 |  1 | paint|       10 | 2015-02-15 10:10:00.000 | 2015-02-15 10:20:00.000 |
                | 2015-02-15 10:15:00 |  1 | paint|       10 | 2015-02-15 10:15:00.000 | 2015-02-15 10:25:00.000 |
                | 2015-02-15 10:24:15 |  2 | paper|        5 | 2015-02-15 10:15:00.000 | 2015-02-15 10:25:00.000 |
                | 2015-02-15 10:24:15 |  2 | paper|        5 | 2015-02-15 10:20:00.000 | 2015-02-15 10:30:00.000 |
                | 2015-02-15 10:24:45 |  3 | brush|       12 | 2015-02-15 10:15:00.000 | 2015-02-15 10:25:00.000 |
                | 2015-02-15 10:24:45 |  3 | brush|       12 | 2015-02-15 10:20:00.000 | 2015-02-15 10:30:00.000 |
                | 2015-02-15 10:58:00 |  4 | paint|        3 | 2015-02-15 10:50:00.000 | 2015-02-15 11:00:00.000 |
                | 2015-02-15 10:58:00 |  4 | paint|        3 | 2015-02-15 10:55:00.000 | 2015-02-15 11:05:00.000 |
                | 2015-02-15 11:10:00 |  5 | paint|        3 | 2015-02-15 11:05:00.000 | 2015-02-15 11:15:00.000 |
                | 2015-02-15 11:10:00 |  5 | paint|        3 | 2015-02-15 11:10:00.000 | 2015-02-15 11:20:00.000 |
                +---------------------+----+---------+-------+-------------------------+-------------------------+
                (10 rows)
                
                SELECT * FROM TABLE(HOP((SELECT * FROM ORDERS), DESCRIPTOR(ROWTIME), INTERVAL '5' MINUTE, INTERVAL '10' MINUTE));
                +---------------------+----+---------+-------+-------------------------+-------------------------+
                | ROWTIME             | ID | PRODUCT | UNITS | window_start            | window_end              |
                +---------------------+----+---------+-------+-------------------------+-------------------------+
                | 2015-02-15 10:15:00 |  1 | paint|       10 | 2015-02-15 10:10:00.000 | 2015-02-15 10:20:00.000 |
                | 2015-02-15 10:15:00 |  1 | paint|       10 | 2015-02-15 10:15:00.000 | 2015-02-15 10:25:00.000 |
                | 2015-02-15 10:24:15 |  2 | paper|        5 | 2015-02-15 10:15:00.000 | 2015-02-15 10:25:00.000 |
                | 2015-02-15 10:24:15 |  2 | paper|        5 | 2015-02-15 10:20:00.000 | 2015-02-15 10:30:00.000 |
                | 2015-02-15 10:24:45 |  3 | brush|       12 | 2015-02-15 10:15:00.000 | 2015-02-15 10:25:00.000 |
                | 2015-02-15 10:24:45 |  3 | brush|       12 | 2015-02-15 10:20:00.000 | 2015-02-15 10:30:00.000 |
                | 2015-02-15 10:58:00 |  4 | paint|        3 | 2015-02-15 10:50:00.000 | 2015-02-15 11:00:00.000 |
                | 2015-02-15 10:58:00 |  4 | paint|        3 | 2015-02-15 10:55:00.000 | 2015-02-15 11:05:00.000 |
                | 2015-02-15 11:10:00 |  5 | paint|        3 | 2015-02-15 11:05:00.000 | 2015-02-15 11:15:00.000 |
                | 2015-02-15 11:10:00 |  5 | paint|        3 | 2015-02-15 11:10:00.000 | 2015-02-15 11:20:00.000 |
                +---------------------+----+---------+-------+-------------------------+-------------------------+
                (10 rows)""");
    }

    @Test
    public void testHop4() {
        this.qs("""
                SELECT * FROM TABLE(HOP(TABLE ORDERS, DESCRIPTOR(ROWTIME), INTERVAL '5' MINUTE, INTERVAL '10' MINUTE, INTERVAL '2' MINUTE));
                +---------------------+----+---------+-------+-------------------------+-------------------------+
                | ROWTIME             | ID | PRODUCT | UNITS | window_start            | window_end              |
                +---------------------+----+---------+-------+-------------------------+-------------------------+
                | 2015-02-15 10:15:00 |  1 | paint|       10 | 2015-02-15 10:07:00.000 | 2015-02-15 10:17:00.000 |
                | 2015-02-15 10:15:00 |  1 | paint|       10 | 2015-02-15 10:12:00.000 | 2015-02-15 10:22:00.000 |
                | 2015-02-15 10:24:15 |  2 | paper|        5 | 2015-02-15 10:17:00.000 | 2015-02-15 10:27:00.000 |
                | 2015-02-15 10:24:15 |  2 | paper|        5 | 2015-02-15 10:22:00.000 | 2015-02-15 10:32:00.000 |
                | 2015-02-15 10:24:45 |  3 | brush|       12 | 2015-02-15 10:17:00.000 | 2015-02-15 10:27:00.000 |
                | 2015-02-15 10:24:45 |  3 | brush|       12 | 2015-02-15 10:22:00.000 | 2015-02-15 10:32:00.000 |
                | 2015-02-15 10:58:00 |  4 | paint|        3 | 2015-02-15 10:52:00.000 | 2015-02-15 11:02:00.000 |
                | 2015-02-15 10:58:00 |  4 | paint|        3 | 2015-02-15 10:57:00.000 | 2015-02-15 11:07:00.000 |
                | 2015-02-15 11:10:00 |  5 | paint|        3 | 2015-02-15 11:02:00.000 | 2015-02-15 11:12:00.000 |
                | 2015-02-15 11:10:00 |  5 | paint|        3 | 2015-02-15 11:07:00.000 | 2015-02-15 11:17:00.000 |
                +---------------------+----+---------+-------+-------------------------+-------------------------+
                (10 rows)""");
    }
}
