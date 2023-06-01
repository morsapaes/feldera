/*
 * Copyright 2022 VMware, Inc.
 * SPDX-License-Identifier: MIT
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the "Software"), to deal
 * in the Software without restriction, including without limitation the rights
 * to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in all
 * copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 * AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 * OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
 * SOFTWARE.
 */

package org.dbsp.sqllogictest.executors;

import net.hydromatic.sqllogictest.OptionsParser;
import net.hydromatic.sqllogictest.SltSqlStatement;
import net.hydromatic.sqllogictest.SltTestFile;
import net.hydromatic.sqllogictest.TestStatistics;
import net.hydromatic.sqllogictest.executors.HsqldbExecutor;
import net.hydromatic.sqllogictest.executors.JdbcExecutor;
import org.dbsp.sqlCompiler.compiler.CompilerOptions;
import org.dbsp.sqlCompiler.compiler.backend.DBSPCompiler;
import org.dbsp.sqlCompiler.ir.expression.DBSPExpression;
import org.dbsp.sqlCompiler.ir.expression.DBSPTupleExpression;
import org.dbsp.sqlCompiler.ir.expression.literal.*;
import org.dbsp.sqlCompiler.ir.type.DBSPType;
import org.dbsp.sqlCompiler.ir.type.DBSPTypeTuple;
import org.dbsp.sqlCompiler.ir.type.primitive.DBSPTypeDouble;
import org.dbsp.sqlCompiler.ir.type.primitive.DBSPTypeInteger;
import org.dbsp.sqlCompiler.ir.type.primitive.DBSPTypeString;
import org.dbsp.sqlCompiler.ir.type.primitive.DBSPTypeWeight;

import javax.annotation.Nullable;
import java.io.IOException;
import java.sql.*;
import java.util.ArrayList;
import java.util.List;
import java.util.Set;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

/**
 * This is a hybrid test executor which keeps all the state in a
 * database using JDBC and executes all queries using DBSP>
 */
public class DbspJdbcExecutor extends DBSPExecutor {
    private final JdbcExecutor statementExecutor;
    private final List<String> tablesCreated;
    // TODO: remove this field.
    private final Connection fakeConnection;

    static int counter = 0;

    /**
     * @param compilerOptions Compilation options.
     * @param executor Executor based on JDBC.
     * @param options  Command-line options.
     */
    public DbspJdbcExecutor(JdbcExecutor executor,
                            OptionsParser.SuppliedOptions options,
                            CompilerOptions compilerOptions) {
        super(options, compilerOptions, "csv");
        this.statementExecutor = executor;
        this.tablesCreated = new ArrayList<>();
        try {
            this.fakeConnection = DriverManager.getConnection("jdbc:hsqldb:mem:db" + counter++, "", "");
        } catch (SQLException e) {
            throw new RuntimeException(e);
        }
    }

    Connection getStatementExecutorConnection() {
        // TODO: once a new version of the org.hydromatic.sql-logic-test package
        // is published replace this with its connection.
        // return this.statementExecutor.getConnection();
        return this.fakeConnection;
    }

    public DBSPZSetLiteral getTableContents(String table) throws SQLException {
        List<DBSPExpression> rows = new ArrayList<>();
        try (Statement stmt1 = this.getStatementExecutorConnection().createStatement()) {
            ResultSet rs = stmt1.executeQuery("SELECT * FROM " + table);
            ResultSetMetaData meta = rs.getMetaData();
            DBSPType[] colTypes = new DBSPType[meta.getColumnCount()];
            for (int i1 = 0; i1 < meta.getColumnCount(); i1++) {
                JDBCType columnType = JDBCType.valueOf(meta.getColumnType(i1 + 1));
                int n = meta.isNullable(i1 + 1);
                boolean nullable;
                if (n == ResultSetMetaData.columnNullable)
                    nullable = true;
                else if (n == ResultSetMetaData.columnNullableUnknown)
                    throw new RuntimeException("Unknown column nullability");
                else
                    nullable = false;
                switch (columnType) {
                    case INTEGER:
                        colTypes[i1] = DBSPTypeInteger.SIGNED_32.setMayBeNull(nullable);
                        break;
                    case REAL:
                    case DOUBLE:
                        colTypes[i1] = DBSPTypeDouble.INSTANCE.setMayBeNull(nullable);
                        break;
                    case VARCHAR:
                    case LONGVARCHAR:
                        colTypes[i1] = DBSPTypeString.INSTANCE.setMayBeNull(nullable);
                        break;
                    default:
                        throw new RuntimeException("Unexpected column type " + columnType);
                }
            }
            while (rs.next()) {
                DBSPExpression[] cols = new DBSPExpression[colTypes.length];
                for (int i = 0; i < colTypes.length; i++) {
                    DBSPExpression exp;
                    DBSPType type = colTypes[i];
                    if (type.is(DBSPTypeInteger.class)) {
                        int value = rs.getInt(i + 1);
                        if (rs.wasNull())
                            exp = DBSPLiteral.none(DBSPTypeInteger.SIGNED_32.setMayBeNull(true));
                        else
                            exp = new DBSPI32Literal(value, type.mayBeNull);
                    } else if (type.is(DBSPTypeDouble.class)) {
                        double value = rs.getDouble(i + 1);
                        if (rs.wasNull())
                            exp = DBSPLiteral.none(DBSPTypeDouble.NULLABLE_INSTANCE);
                        else
                            exp = new DBSPDoubleLiteral(value, type.mayBeNull);
                    } else {
                        String s = rs.getString(i + 1);
                        if (s == null)
                            exp = DBSPLiteral.none(DBSPTypeString.NULLABLE_INSTANCE);
                        else
                            exp = new DBSPStringLiteral(s, type.mayBeNull);
                    }
                    cols[i] = exp;
                }
                DBSPTupleExpression row = new DBSPTupleExpression(cols);
                rows.add(row);
            }
            rs.close();
            if (rows.size() == 0)
                return new DBSPZSetLiteral(new DBSPTypeTuple(colTypes), DBSPTypeWeight.INSTANCE);
            return new DBSPZSetLiteral(DBSPTypeWeight.INSTANCE, rows.toArray(new DBSPExpression[0]));
        }
    }

    @Override
    public TableValue[] getInputSets(DBSPCompiler compiler) throws SQLException {
        TableValue[] result = new TableValue[this.tablesCreated.size()];
        int i = 0;
        for (String table: this.tablesCreated) {
            DBSPZSetLiteral lit = this.getTableContents(table);
            result[i++] = new TableValue(table, lit);
        }
        return result;
    }

    static final String regexCreate = "create\\s+table\\s+(\\w+)";
    static final Pattern patCreate = Pattern.compile(regexCreate);
    static final String regexDrop = "drop\\s+table\\s+(\\w+)";
    static final Pattern patDrop = Pattern.compile(regexDrop);

    /*
     Calcite cannot parse DDL statements in all dialects.
     For example, it has no support for MySQL CREATE TABLE statements
     which indicate the primary key for each column.
     So to handle these we let JDBC execute the statement, then
     we retrieve the table schema and make up a new statement
     in a Calcite-friendly syntax.  This implementation does not
     preserve primary keys, but this does not seem important right now.
     */
    public String generateCreateStatement(String table) throws SQLException {
        StringBuilder builder = new StringBuilder();
        builder.append("CREATE TABLE ");
        builder.append(table);
        builder.append("(");

        try (Statement stmt = this.getStatementExecutorConnection().createStatement()) {
            ResultSet rs = stmt.executeQuery("SELECT * FROM " + table + " WHERE 1 = 0");
            ResultSetMetaData meta = rs.getMetaData();
            for (int i = 0; i < meta.getColumnCount(); i++) {
                JDBCType columnType = JDBCType.valueOf(meta.getColumnType(i + 1));
                int n = meta.isNullable(i + 1);
                String colName = meta.getColumnName(i + 1);

                if (i > 0)
                    builder.append(", ");
                builder.append(colName);
                builder.append(" ");

                boolean nullable;
                if (n == ResultSetMetaData.columnNullable)
                    nullable = true;
                else if (n == ResultSetMetaData.columnNullableUnknown)
                    throw new RuntimeException("Unknown column nullability");
                else
                    nullable = false;
                switch (columnType) {
                    case INTEGER:
                        builder.append("INTEGER");
                        break;
                    case REAL:
                    case DOUBLE:
                        builder.append("DOUBLE");
                        break;
                    case VARCHAR:
                    case LONGVARCHAR:
                        builder.append("VARCHAR");
                        break;
                    default:
                        throw new RuntimeException("Unexpected column type " + columnType);
                }
                if (!nullable)
                    builder.append(" NOT NULL");
            }
            rs.close();
            builder.append(")");
            return builder.toString();
        }
    }

    @Nullable
    String rewriteCreateTable(String command) throws SQLException {
        Matcher m = patCreate.matcher(command);
        if (!m.find())
            return null;
        String tableName = m.group(1);
        this.tablesCreated.add(tableName);
        return this.generateCreateStatement(tableName);
    }

    public boolean statement(SltSqlStatement statement) throws SQLException {
        this.statementExecutor.statement(statement);
        String command = statement.statement.toLowerCase();
        this.options.message("Executing " + command + "\n", 2);
        @Nullable
        String create = this.rewriteCreateTable(command);
        if (create != null) {
            SltSqlStatement rewritten = new SltSqlStatement(create, statement.shouldPass);
            super.statement(rewritten);
        } else if (command.contains("drop table") ||
                command.contains("create view") ||
                command.contains("drop view")) {
            super.statement(statement);
            Matcher m = patDrop.matcher(command);
            if (m.find()) {
                String tableName = m.group(1);
                this.tablesCreated.remove(tableName);
            }
        }
        return true;
    }

    @Override
    void reset() {
        this.tablesCreated.clear();
        super.reset();
    }

    @Override
    public TestStatistics execute(SltTestFile file, OptionsParser.SuppliedOptions options)
            throws SQLException {
        this.statementExecutor.establishConnection();
        this.statementExecutor.dropAllViews();
        this.statementExecutor.dropAllTables();
        return super.execute(file, options);
    }

    public static void register(OptionsParser parser) {
        parser.registerExecutor("hybrid", () -> {
            OptionsParser.SuppliedOptions options = parser.getOptions();
            try {
                // TODO: replace there on the next version of sql-logic-test.
                // JdbcExecutor inner = parser.getExecutorByName("hsql");
                JdbcExecutor inner = new HsqldbExecutor(options);
                // DBSPExecutor unused = parser.getExecutorByName("dbsp").to(DBSPExecutor.class);
                // boolean validateJIT = unused.validateJIT;
                boolean validateJIT = false;
                // CompilerOptions compilerOptions = unused.compilerOptions;
                CompilerOptions compilerOptions = new CompilerOptions();
                DbspJdbcExecutor result = new DbspJdbcExecutor(
                        inner, options, compilerOptions);
                Set<String> bugs = options.readBugsFile();
                result.avoid(bugs);
                return result;
            } catch (IOException e) {
                throw new RuntimeException(e);
            }
        });
    }
}